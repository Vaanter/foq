use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, trace};

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::commands::r#impl::shared::get_listing_or_error_reply;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::io::entry_data::EntryType;
use crate::session::command_processor::CommandProcessor;

pub(crate) struct List;

#[async_trait]
impl Executable for List {
  #[tracing::instrument(skip(command_processor, reply_sender))]
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::LIST);

    let session_properties = command_processor.session_properties.read().await;

    let path = if command.argument == "-a" {
      "."
    } else {
      &command.argument
    };

    let listing = session_properties
      .file_system_view_root
      .list_dir(path);

    let listing = match get_listing_or_error_reply(listing) {
      Ok(l) => l,
      Err(r) => return Self::reply(r, reply_sender).await
    };

    debug!("Locking data stream!");
    let stream = command_processor
      .data_wrapper
      .lock()
      .await
      .get_data_stream()
      .await;

    match stream.lock().await.as_mut() {
      Some(s) => {
        let mem = listing
          .iter()
          .filter(|l| l.entry_type() != EntryType::CDIR)
          .map(|l| l.to_list_string())
          .collect::<String>();
        trace!(
          "Sending listing to client:\n{}",
          mem.replace("\r\n", "\\r\\n")
        );
        let len = s.write_all(mem.as_ref()).await;
        debug!("Sending listing result: {:?}", len);
      }
      None => {
        info!("Data stream is not open!");
        Self::reply(
          Reply::new(
            ReplyCode::BadSequenceOfCommands,
            "Data connection is not open!",
          ),
          reply_sender,
        )
        .await;
        return;
      }
    }

    Self::reply(
      Reply::new(
        ReplyCode::FileStatusOkay,
        "Transferring directory information!",
      ),
      reply_sender,
    )
    .await;

    debug!("Listing sent to client!");
    Self::reply(
      Reply::new(
        ReplyCode::ClosingDataConnection,
        "Directory information sent!",
      ),
      reply_sender,
    )
    .await;
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
  use std::sync::Arc;
  use std::time::Duration;

  use regex::Regex;
  use tokio::io::AsyncReadExt;
  use tokio::net::TcpStream;
  use tokio::sync::mpsc::channel;
  use tokio::sync::{Mutex, RwLock};
  use tokio::time::timeout;

  use crate::auth::user_permission::UserPermission;
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::list::List;
  use crate::commands::reply_code::ReplyCode;
  use crate::data_channels::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::file_system_view::FileSystemView;
  use crate::session::command_processor::CommandProcessor;
  use crate::session::session_properties::SessionProperties;
  use crate::utils::test_utils::{receive_and_verify_reply, TestReplySender, LOCALHOST};

  #[tokio::test]
  async fn simple_listing_tcp() {
    let command = Command::new(Commands::LIST, String::new());

    let permissions = HashSet::from([UserPermission::READ, UserPermission::LIST]);
    let root_path = current_dir().unwrap().join("test_files");
    let label = "test_files";
    let view = FileSystemView::new(root_path.clone(), label.clone(), permissions.clone());

    let mut session_properties = SessionProperties::new();
    session_properties
      .file_system_view_root
      .set_views(vec![view]);
    assert!(session_properties
      .file_system_view_root
      .change_working_directory(label.clone()).unwrap());

    let session_properties = Arc::new(RwLock::new(session_properties));
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);
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

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      List::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::FileStatusOkay, None).await;

    let mut buffer = [0; 2048];
    match timeout(Duration::from_secs(5), client_dc.read(&mut buffer)).await {
      Ok(Ok(len)) => {
        let msg = String::from_utf8_lossy(&buffer[..len]);
        assert!(!msg.is_empty());

        let file_count = root_path
          .read_dir()
          .expect("Failed to read current path!")
          .count();

        println!("Message:\n{}", msg);

        let re = Regex::new(r"^[dl-](?:[r-][w-][x-]){3} 1 user group  {0,12}\d{1,20} [A-Za-z]{3} [0-3][0-9] [012][0-4]:[0-5][0-9] .*$").unwrap();

        for line in msg.lines() {
          assert!(re.is_match(line), "Invalid line: '{line}'");
        }
        assert_eq!(file_count, msg.lines().count());
      }
      Ok(Err(e)) => {
        panic!("Transfer error: {}", e);
      }
      Err(_) => {
        panic!("Transfer timed out.");
      }
    };

    receive_and_verify_reply(2, &mut rx, ReplyCode::ClosingDataConnection, None).await;
  }

  #[tokio::test]
  async fn not_logged_in_test() {
    let command = Command::new(Commands::LIST, String::new());

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));

    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(_) = timeout(
      Duration::from_secs(3),
      List::execute(&mut command_processor, &command, &mut reply_sender),
    )
      .await
    {
      panic!("Command timeout!");
    };
    receive_and_verify_reply(2, &mut rx, ReplyCode::NotLoggedIn, None).await;
  }

  #[tokio::test]
  async fn not_directory_test() {
    let command = Command::new(Commands::LIST, String::from("1MiB.txt"));

    let permissions = HashSet::from([UserPermission::READ, UserPermission::LIST]);
    let root_path = current_dir().unwrap().join("test_files");
    let label = "test_files";
    let view = FileSystemView::new(root_path.clone(), label.clone(), permissions.clone());

    let mut session_properties = SessionProperties::new();
    session_properties
      .file_system_view_root
      .set_views(vec![view]);
    assert!(session_properties
      .file_system_view_root
      .change_working_directory(label.clone()).unwrap());
    let _ = session_properties.username.insert("test".to_string());

    let session_properties = Arc::new(RwLock::new(session_properties));
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(_) = timeout(
      Duration::from_secs(3),
      List::execute(&mut command_processor, &command, &mut reply_sender),
    )
      .await
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(
      2,
      &mut rx,
      ReplyCode::SyntaxErrorInParametersOrArguments,
      None,
    )
      .await;
  }

  #[tokio::test]
  async fn nonexistent_test() {
    let command = Command::new(Commands::LIST, String::from("NONEXISTENT"));

    let permissions = HashSet::from([UserPermission::READ, UserPermission::LIST]);
    let root_path = current_dir().unwrap().join("test_files");
    let label = "test_files";
    let view = FileSystemView::new(root_path.clone(), label.clone(), permissions.clone());

    let mut session_properties = SessionProperties::new();
    session_properties
      .file_system_view_root
      .set_views(vec![view]);
    let _ = session_properties.username.insert("test".to_string());

    let session_properties = Arc::new(RwLock::new(session_properties));
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      List::execute(&mut command_processor, &command, &mut reply_sender),
    )
      .await
      .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::FileUnavailable, None).await;
  }

  #[tokio::test]
  async fn insufficient_permissions_test() {
    let command = Command::new(Commands::LIST, String::new());

    let permissions = HashSet::from([]);
    let root_path = current_dir().unwrap().join("test_files");
    let label = "test_files";
    let view = FileSystemView::new(root_path.clone(), label.clone(), permissions.clone());

    let mut session_properties = SessionProperties::new();
    session_properties
      .file_system_view_root
      .set_views(vec![view]);
    assert!(session_properties
      .file_system_view_root
      .change_working_directory(label.clone()).unwrap());
    let _ = session_properties.username.insert("test".to_string());

    let session_properties = Arc::new(RwLock::new(session_properties));
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(_) = timeout(
      Duration::from_secs(3),
      List::execute(&mut command_processor, &command, &mut reply_sender),
    )
      .await
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(
      2,
      &mut rx,
      ReplyCode::FileUnavailable,
      Some("Insufficient permissions!"),
    )
      .await;
  }

  #[tokio::test]
  async fn data_channel_not_open_tcp() {
    let command = Command::new(Commands::LIST, String::new());

    let permissions = HashSet::from([UserPermission::READ, UserPermission::LIST]);
    let root_path = current_dir().unwrap().join("test_files");
    let label = "test_files";
    let view = FileSystemView::new(root_path.clone(), label.clone(), permissions.clone());

    let mut session_properties = SessionProperties::new();
    session_properties
      .file_system_view_root
      .set_views(vec![view]);
    let _ = session_properties.username.insert("test".to_string());

    let session_properties = Arc::new(RwLock::new(session_properties));
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      List::execute(&mut command_processor, &command, &mut reply_sender),
    )
      .await
      .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::BadSequenceOfCommands, None).await;
  }
}
