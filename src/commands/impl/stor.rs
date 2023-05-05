use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, warn};

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::commands::r#impl::shared::{
  get_data_channel_lock, get_open_file_result, get_transfer_reply,
};
use crate::handlers::reply_sender::ReplySend;
use crate::io::command_processor::CommandProcessor;
use crate::io::open_options_flags::OpenOptionsWrapperBuilder;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;

pub(crate) struct Stor;

#[async_trait]
impl Executable for Stor {
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::STOR);

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
    let success = match tokio::io::copy(&mut data_channel.as_mut().unwrap(), &mut file).await {
      Ok(len) => {
        debug!("Received {len} bytes.");
        file.flush().await.is_ok()
      }
      Err(e) => {
        warn!("Error sending file! {}", e);
        false
      }
    };

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
  use std::fs::remove_file;
  use std::net::SocketAddr;
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
  use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::command_processor::CommandProcessor;
  use crate::io::file_system_view::FileSystemView;
  use crate::io::reply_code::ReplyCode;
  use crate::io::session_properties::SessionProperties;
  use crate::utils::test_utils::{
    open_tcp_data_channel, receive_and_verify_reply, TestReplySender, LOCALHOST,
  };

  async fn common(local_file: &'static str, remote_file: &str) {
    if !Path::new(&local_file).exists() {
      panic!("Test file does not exist! Cannot proceed!");
    }
    println!("Remote file: {:?}", temp_dir().join(&remote_file));

    let _cleanup = Cleanup { 0: &remote_file };

    let command = Command::new(Commands::STOR, remote_file);

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));

    let label = "test";
    let view = FileSystemView::new(
      temp_dir(),
      label.clone(),
      HashSet::from([
        UserPermission::READ,
        UserPermission::CREATE,
        UserPermission::WRITE,
      ]),
    );

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
    session_properties
      .write()
      .await
      .file_system_view_root
      .change_working_directory(label);
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let mut command_processor = CommandProcessor::new(session_properties.clone(), wrapper);

    let mut client_dc = open_tcp_data_channel(&mut command_processor).await;

    // TODO adjust timeout better maybe?
    let timeout_secs = 1
      + Path::new(&local_file)
        .metadata()
        .expect("Metadata should be accessible!")
        .len()
        .ilog10();

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);

    let command_fut = tokio::spawn(async move {
      timeout(
        Duration::from_secs(timeout_secs as u64),
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

      let remote = temp_dir().join(&remote_file);
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

        let transfer_len = match client_dc.write(&mut local_buffer).await {
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

    match timeout(Duration::from_secs(5), transfer).await {
      Ok(()) => println!("Transfer complete!"),
      Err(_) => panic!("Transfer timed out!"),
    }
  }

  // Removes the temp file used in tests when dropped
  struct Cleanup<'a>(&'a str);

  impl<'a> Drop for Cleanup<'a> {
    fn drop(&mut self) {
      if let Err(e) = remove_file(temp_dir().join(self.0)) {
        eprintln!("Failed to remove: {}, {}", self.0, e);
      };
    }
  }

  #[tokio::test]
  async fn two_kib_test() {
    const LOCAL_FILE: &'static str = "test_files/2KiB.txt";
    let remote_file = format!("{}.txt", Uuid::new_v4().as_hyphenated());
    common(LOCAL_FILE, &remote_file).await;
  }

  #[tokio::test]
  async fn test_one_mib() {
    const LOCAL_FILE: &'static str = "test_files/1MiB.txt";
    let remote_file = format!("{}.txt", Uuid::new_v4().as_hyphenated());
    common(LOCAL_FILE, &remote_file).await;
  }

  #[tokio::test]
  async fn test_ten_paragraphs() {
    const LOCAL_FILE: &'static str = "test_files/lorem_10_paragraphs.txt";
    let remote_file = format!("{}.txt", Uuid::new_v4().as_hyphenated());
    common(LOCAL_FILE, &remote_file).await;
  }

  #[tokio::test]
  async fn not_logged_in_test() {
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let _ = open_tcp_data_channel(&mut command_processor).await;

    let command = Command::new(Commands::STOR, "NONEXISTENT");
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
      label.clone(),
      HashSet::from([
        UserPermission::READ,
        UserPermission::WRITE,
        UserPermission::CREATE,
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

    let command = Command::new(Commands::STOR, format!("NONEXISTENT"));
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
      label.clone(),
      HashSet::from([
        UserPermission::READ,
        UserPermission::WRITE,
        UserPermission::CREATE,
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

    let command = Command::new(Commands::STOR, "");
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
