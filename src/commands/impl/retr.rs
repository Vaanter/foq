use async_trait::async_trait;

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

pub(crate) struct Retr;

#[async_trait]
impl Executable for Retr {
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::RETR);

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
      .read(true)
      .build()
      .unwrap();
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

    let success = match tokio::io::copy(&mut file, &mut data_channel.as_mut().unwrap()).await {
      Ok(len) => {
        println!("Sent {len} bytes.");
        true
      }
      Err(_) => {
        eprintln!("Error sending file!");
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
  use std::env::current_dir;
  use std::net::SocketAddr;
  use std::path::Path;
  use std::sync::Arc;
  use std::time::Duration;

  use blake3::Hasher;
  use tokio::fs::OpenOptions;
  use tokio::io::AsyncReadExt;
  use tokio::net::TcpStream;
  use tokio::sync::mpsc::channel;
  use tokio::sync::{Mutex, RwLock};
  use tokio::time::timeout;

  use crate::auth::user_permission::UserPermission;
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::retr::Retr;
  use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::command_processor::CommandProcessor;
  use crate::io::file_system_view::FileSystemView;
  use crate::io::reply_code::ReplyCode;
  use crate::io::session_properties::SessionProperties;
  use crate::utils::test_utils::{receive_and_verify_reply, TestReplySender};

  async fn common(file_name: &'static str) {
    if !Path::new(&file_name).exists() {
      panic!("Test file does not exist! Cannot proceed!");
    }

    let command = Command::new(Commands::RETR, file_name);

    let label = "test";
    let view = FileSystemView::new(
      current_dir().unwrap(),
      label.clone(),
      HashSet::from([UserPermission::READ]),
    );

    let mut session_properties = SessionProperties::new();
    session_properties
      .file_system_view_root
      .set_views(vec![view]);
    session_properties
      .file_system_view_root
      .change_working_directory(label);
    let _ = session_properties.username.insert("test".to_string());

    let ip: SocketAddr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");
    let session_properties = Arc::new(RwLock::new(session_properties));
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let mut command_processor = CommandProcessor::new(session_properties.clone(), wrapper);
    let addr = match command_processor
      .data_wrapper
      .clone()
      .lock()
      .await
      .open_data_stream()
      .await
    {
      Ok(addr) => addr,
      Err(_) => panic!("Failed to open passive data listener!"),
    };

    println!("Connecting to passive listener");
    let mut client_dc = match TcpStream::connect(addr).await {
      Ok(c) => c,
      Err(e) => {
        panic!("Client passive connection failed: {}", e);
      }
    };
    println!("Client passive connection successful!");

    // TODO adjust timeout better maybe?
    let timeout_secs = 1
      + Path::new(&file_name)
        .metadata()
        .expect("Metadata should be accessible!")
        .len()
        .ilog10();

    let _ = command_processor
      .data_wrapper
      .lock()
      .await
      .get_data_stream()
      .await
      .lock()
      .await;
    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);

    let command_fut = tokio::spawn(async move {
      if let Err(e) = timeout(
        Duration::from_secs(timeout_secs as u64),
        Retr::execute(&mut command_processor, &command, &mut reply_sender),
      )
      .await
      {
        panic!("Command timeout!");
      };
    });

    receive_and_verify_reply(2, &mut rx, ReplyCode::FileStatusOkay, None).await;

    let transfer = async move {
      let mut local_file_hasher = Hasher::new();
      let mut sent_file_hasher = Hasher::new();

      let mut local_file = OpenOptions::new()
        .read(true)
        .open(&file_name)
        .await
        .expect("Test file must exist!");

      let mut transfer_buffer = [0; 1024];
      let mut local_buffer = [0; 1024];
      loop {
        let transfer_len = match client_dc.read(&mut transfer_buffer).await {
          Ok(len) => len,
          Err(e) => panic!("File transfer failed! {e}"),
        };
        let local_len = match local_file.read(&mut local_buffer).await {
          Ok(len) => len,
          Err(e) => panic!("Failed to read local file! {e}"),
        };

        assert_eq!(transfer_len, local_len);
        if local_len == 0 || transfer_len == 0 {
          break;
        }

        sent_file_hasher.update(&transfer_buffer[..transfer_len]);
        local_file_hasher.update(&local_buffer[..local_len]);
      }
      assert_eq!(
        local_file_hasher.finalize(),
        sent_file_hasher.finalize(),
        "File hashes do not match!"
      );
      println!("File hashes match!");
    };

    match timeout(Duration::from_secs(5), transfer).await {
      Ok(()) => println!("Transfer complete!"),
      Err(e) => panic!("Transfer timed out!"),
    }

    receive_and_verify_reply(2, &mut rx, ReplyCode::ClosingDataConnection, None).await;

    command_fut.await.expect("Command should complete!");
  }

  #[tokio::test]
  async fn test_two_kib() {
    const FILE_NAME: &'static str = "test_files/2KiB.txt";
    common(FILE_NAME).await;
  }

  #[tokio::test]
  async fn test_one_mib() {
    const FILE_NAME: &'static str = "test_files/1MiB.txt";
    common(FILE_NAME).await;
  }

  #[tokio::test]
  async fn test_ten_paragraphs() {
    const FILE_NAME: &'static str = "test_files/lorem_10_paragraphs.txt";
    common(FILE_NAME).await;
  }

  #[tokio::test]
  async fn not_logged_in_test() {
    let ip: SocketAddr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let command = Command::new(Commands::RETR, "NONEXISTENT");
    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      Retr::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::NotLoggedIn, None).await;
  }

  #[tokio::test]
  async fn data_channel_not_open_test() {
    let ip: SocketAddr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");

    let label = "test";
    let view = FileSystemView::new(
      current_dir().unwrap(),
      label.clone(),
      HashSet::from([UserPermission::READ]),
    );

    let mut session_properties = SessionProperties::new();
    session_properties
      .file_system_view_root
      .set_views(vec![view]);
    let _ = session_properties.username.insert("test".to_string());

    let session_properties = Arc::new(RwLock::new(session_properties));
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let command = Command::new(
      Commands::RETR,
      format!("{}/test_files/1MiB.txt", label.clone()),
    );
    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      Retr::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::BadSequenceOfCommands, None).await;
  }
}
