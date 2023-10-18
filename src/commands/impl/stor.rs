use async_trait::async_trait;
use tracing::{debug, info};

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::commands::r#impl::shared::{
  get_data_channel_lock, get_open_file_result, get_transfer_reply, transfer_data,
};
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::io::open_options_flags::OpenOptionsWrapperBuilder;
use crate::session::command_processor::CommandProcessor;

pub(crate) struct Stor;

#[async_trait]
impl Executable for Stor {
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::Stor);

    if command.argument.is_empty() {
      Self::reply(
        Reply::new(
          ReplyCode::SyntaxErrorInParametersOrArguments,
          "No file specified!",
        ),
        reply_sender,
      )
      .await;
      return;
    }

    let session_properties = command_processor.session_properties.read().await;

    if !session_properties.is_logged_in() {
      Self::reply(
        Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"),
        reply_sender,
      )
      .await;
      return;
    }

    let data_channel_lock = get_data_channel_lock(command_processor.data_wrapper.clone()).await;
    let mut data_channel = match data_channel_lock {
      Ok(dc) => dc,
      Err(e) => {
        Self::reply(e, reply_sender).await;
        return;
      }
    };

    let options = OpenOptionsWrapperBuilder::default()
      .write(true)
      .truncate(true)
      .create(true)
      .build()
      .unwrap();
    info!(
      "User '{}' opening file '{}'.",
      session_properties.username.as_ref().unwrap(),
      &command.argument
    );
    let file = session_properties
      .file_system_view_root
      .open_file(&command.argument, options)
      .await;

    let mut file = match get_open_file_result(file) {
      Ok(f) => f,
      Err(reply) => {
        Self::reply(reply, reply_sender).await;
        return;
      }
    };

    Self::reply(
      Reply::new(ReplyCode::FileStatusOkay, "Starting file transfer!"),
      reply_sender,
    )
    .await;

    debug!("Receiving file data!");
    let mut buffer = vec![0; 4096];
    let success = transfer_data(&mut data_channel.as_mut().unwrap(), &mut file, &mut buffer).await;

    Self::reply(get_transfer_reply(success), reply_sender).await;

    // Needed to release the lock. Maybe find a better way to do this?
    drop(data_channel);
    command_processor
      .data_wrapper
      .lock()
      .await
      .close_data_stream()
      .await;
  }
}

#[cfg(test)]
mod tests {
  use std::collections::HashSet;
  use std::env::{current_dir, temp_dir};
  use std::path::Path;
  use std::sync::Arc;
  use std::time::Duration;

  use blake3::Hasher;
  use tokio::fs::OpenOptions;
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
  use tokio::sync::mpsc::channel;
  use tokio::sync::{Mutex, RwLock};
  use tokio::time::timeout;
  use uuid::Uuid;

  use crate::auth::user_permission::UserPermission;
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::stor::Stor;
  use crate::commands::reply_code::ReplyCode;
  use crate::data_channels::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::file_system_view::FileSystemView;
  use crate::session::command_processor::CommandProcessor;
  use crate::session::session_properties::SessionProperties;
  use crate::utils::test_utils::{
    generate_test_file, open_tcp_data_channel, receive_and_verify_reply,
    setup_test_command_processor_custom, CommandProcessorSettingsBuilder, TempFileCleanup,
    TestReplySender, LOCALHOST,
  };

  async fn common(local_file: &str, remote_file: &str) {
    if !Path::new(&local_file).exists() {
      panic!("Test file does not exist! Cannot proceed!");
    }
    println!("Remote file: {:?}", temp_dir().join(remote_file));

    let _cleanup = TempFileCleanup::new(remote_file);

    let command = Command::new(Commands::Stor, remote_file);

    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(temp_dir())
      .build()
      .expect("Settings should be valid");

    let mut command_processor = setup_test_command_processor_custom(&settings);

    let mut client_dc = open_tcp_data_channel(&mut command_processor).await;

    const TIMEOUT_SECS: u64 = 300;

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);

    let command_fut = tokio::spawn(async move {
      timeout(
        Duration::from_secs(TIMEOUT_SECS),
        Stor::execute(&mut command_processor, &command, &mut reply_sender),
      )
      .await
      .expect("Command timeout!");
    });

    receive_and_verify_reply(2, &mut rx, ReplyCode::FileStatusOkay, None).await;

    let transfer = async move {
      let mut local_file_hasher = Hasher::new();
      let mut remote_file_hasher = Hasher::new();

      let mut local_file = OpenOptions::new()
        .read(true)
        .open(&local_file)
        .await
        .expect("Local test file must exist!");

      let remote = temp_dir().join(remote_file);
      let mut remote_file = OpenOptions::new()
        .read(true)
        .open(remote)
        .await
        .expect("Remote test file must exist!");

      const REMOTE_BUFFER_SIZE: usize = 1024;
      const LOCAL_BUFFER_SIZE: usize = 1024;
      let mut remote_buffer = [0; REMOTE_BUFFER_SIZE];
      let mut local_buffer = [0; LOCAL_BUFFER_SIZE];

      let mut sends = 0;
      let mut reads = 0;
      loop {
        let local_len = match local_file.read(&mut local_buffer).await {
          Ok(len) => len,
          Err(e) => panic!("Failed to read local file! {e}"),
        };

        if local_len == 0 {
          break;
        }

        let transfer_len = match client_dc.write(&local_buffer).await {
          Ok(len) => len,
          Err(e) => panic!("File transfer failed! {e}"),
        };
        sends += transfer_len;
        let _ = client_dc.flush().await;

        local_file_hasher.update(&local_buffer[..transfer_len]);

        remote_buffer.fill(0);
      }

      assert!(client_dc.shutdown().await.is_ok());

      receive_and_verify_reply(2, &mut rx, ReplyCode::ClosingDataConnection, None).await;
      command_fut.await.expect("Command should complete!");

      loop {
        let remote_len = match remote_file.read(&mut remote_buffer).await {
          Ok(len) => len,
          Err(e) => panic!("Failed to read remote file! {e}"),
        };

        if remote_len == 0 {
          break;
        }
        reads += remote_len;
        remote_file_hasher.update(&remote_buffer[..remote_len]);
        remote_buffer.fill(0);
      }

      println!("Read: {reads}, Sent: {sends}");

      assert_eq!(
        local_file_hasher.finalize(),
        remote_file_hasher.finalize(),
        "File hashes do not match!"
      );
      println!("File hashes match!");
    };

    match timeout(Duration::from_secs(TIMEOUT_SECS), transfer).await {
      Ok(()) => println!("Transfer complete!"),
      Err(_) => panic!("Transfer timed out!"),
    }
  }

  #[tokio::test]
  async fn two_kib_test() {
    const LOCAL_FILE: &str = "test_files/2KiB.txt";
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common(LOCAL_FILE, &remote_file).await;
  }

  #[tokio::test]
  async fn test_one_mib() {
    const LOCAL_FILE: &str = "test_files/1MiB.txt";
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common(LOCAL_FILE, &remote_file).await;
  }

  #[tokio::test]
  #[ignore]
  async fn one_gib_test() {
    let file_path = temp_dir().join(format!("{}.test", Uuid::new_v4()));
    let file_path_str = file_path.to_str().unwrap();
    let _cleanup = TempFileCleanup::new(file_path_str);
    generate_test_file((2u64.pow(30)) as usize, Path::new(file_path_str)).await;
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common(file_path_str, &remote_file).await;
  }

  #[tokio::test]
  async fn test_ten_paragraphs() {
    const LOCAL_FILE: &str = "test_files/lorem_10_paragraphs.txt";
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common(LOCAL_FILE, &remote_file).await;
  }

  #[tokio::test]
  async fn not_logged_in_test() {
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let _ = open_tcp_data_channel(&mut command_processor).await;

    let command = Command::new(Commands::Stor, "NONEXISTENT");
    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      Stor::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::NotLoggedIn, None).await;
  }

  #[tokio::test]
  async fn data_channel_not_open_test() {
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));

    let label = "test";
    let view = FileSystemView::new(
      current_dir().unwrap(),
      label,
      HashSet::from([
        UserPermission::Read,
        UserPermission::Write,
        UserPermission::Create,
      ]),
    );

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    session_properties
      .write()
      .await
      .file_system_view_root
      .set_views(vec![view]);
    let _ = session_properties
      .write()
      .await
      .username
      .insert("test".to_string());
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let command = Command::new(Commands::Stor, "NONEXISTENT".to_string());
    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      Stor::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::BadSequenceOfCommands, None).await;
  }

  #[tokio::test]
  async fn no_file_specified_test() {
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));

    let label = "test";
    let view = FileSystemView::new(
      current_dir().unwrap(),
      label,
      HashSet::from([
        UserPermission::Read,
        UserPermission::Write,
        UserPermission::Create,
      ]),
    );

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    session_properties
      .write()
      .await
      .file_system_view_root
      .set_views(vec![view]);
    let _ = session_properties
      .write()
      .await
      .username
      .insert("test".to_string());
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let _ = open_tcp_data_channel(&mut command_processor).await;

    let command = Command::new(Commands::Stor, "");
    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      Stor::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(
      2,
      &mut rx,
      ReplyCode::SyntaxErrorInParametersOrArguments,
      None,
    )
    .await;
  }
}
