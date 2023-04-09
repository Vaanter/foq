use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::command_processor::CommandProcessor;
use crate::io::error::Error;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;

#[derive(Copy, Clone, Eq, PartialEq, Default)]
pub(crate) struct Mlsd;

#[async_trait]
impl Executable for Mlsd {
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::MLSD);

    println!("Getting listing!");
    let session_properties = command_processor.session_properties.read().await;
    let listing = session_properties
      .file_system_view_root
      .list_dir(&command.argument);

    let listing = match listing {
      Ok(l) => l,
      Err(Error::UserError) => {
        Self::reply(
          Reply::new(ReplyCode::NotLoggedIn, Error::UserError.to_string()),
          reply_sender,
        )
        .await;
        return;
      }
      Err(Error::OsError(_)) | Err(Error::SystemError) => {
        Self::reply(
          Reply::new(
            ReplyCode::RequestedActionAborted,
            "Requested action aborted: local error in processing.",
          ),
          reply_sender,
        )
        .await;
        return;
      }
      Err(Error::NotADirectoryError) => {
        Self::reply(
          Reply::new(
            ReplyCode::SyntaxErrorInParametersOrArguments,
            Error::NotADirectoryError.to_string(),
          ),
          reply_sender,
        )
        .await;
        return;
      }
      Err(Error::PermissionError) => {
        Self::reply(
          Reply::new(
            ReplyCode::FileUnavailable,
            Error::PermissionError.to_string(),
          ),
          reply_sender,
        )
        .await;
        return;
      }
      Err(Error::NotFoundError(message)) | Err(Error::InvalidPathError(message)) => {
        Self::reply(
          Reply::new(ReplyCode::FileUnavailable, message),
          reply_sender,
        )
        .await;
        return;
      }
      Err(_) => unreachable!(),
    };

    println!("Getting data stream");
    let stream = command_processor
      .data_wrapper
      .lock()
      .await
      .get_data_stream()
      .await;

    match stream.lock().await.as_mut() {
      Some(s) => {
        Mlsd::reply(
          Reply::new(
            ReplyCode::FileStatusOkay,
            "Transferring directory information!",
          ),
          reply_sender,
        )
        .await;
        let mem = listing.iter().map(|l| l.to_string()).collect::<String>();
        println!("Writing to data stream");
        let _ = s.write_all(mem.as_ref()).await;
      }
      None => {
        eprintln!("Data stream non existent!");
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

    println!("Written to data stream");
    command_processor
      .data_wrapper
      .lock()
      .await
      .close_data_stream()
      .await;
    Mlsd::reply(
      Reply::new(
        ReplyCode::ClosingDataConnection,
        "Directory information sent!",
      ),
      reply_sender,
    )
    .await;
  }
}

#[cfg(test)]
mod tests {
  use std::collections::HashSet;
  use std::sync::Arc;
  use std::time::Duration;

  use tokio::io::AsyncReadExt;
  use tokio::net::TcpStream;
  use tokio::sync::mpsc::channel;
  use tokio::sync::{Mutex, RwLock};
  use tokio::time::timeout;

  use crate::auth::user_data::UserData;
  use crate::auth::user_permission::UserPermission;
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::mlsd::Mlsd;
  use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::command_processor::CommandProcessor;
  use crate::io::file_system_view::FileSystemView;
  use crate::io::reply_code::ReplyCode;
  use crate::io::session_properties::SessionProperties;
  use crate::utils::test_utils::TestReplySender;

  #[tokio::test]
  async fn simple_listing_tcp() {
    let ip = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");

    let command = Command::new(Commands::MLSD, String::new());

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));

    let permissions = HashSet::from([UserPermission::READ, UserPermission::LIST]);
    let root_path = std::env::current_dir().unwrap().join("test_files");
    let label = "test_files";
    let view = FileSystemView::new(root_path.clone(), label.clone(), permissions.clone());
    let mut user_data = UserData::new("test", "test");
    user_data.add_view(view);

    session_properties.write().await.login(user_data);

    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
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
    if let Err(e) = timeout(
      Duration::from_secs(3),
      Mlsd::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    {
      panic!("Command timeout!");
    };

    let dc_fut = tokio::spawn(async move {
      let mut buffer = [0; 1024];
      match timeout(Duration::from_secs(5), client_dc.read(&mut buffer)).await {
        Ok(Ok(len)) => {
          let msg = String::from_utf8_lossy(&buffer[..len]);
          assert!(!msg.is_empty());

          let file_count = root_path
            .read_dir()
            .expect("Failed to read current path!")
            .count()
            + 1; // Add 1 to account for current path (.)
          assert_eq!(file_count, msg.lines().count());
        }
        Ok(Err(e)) => {
          assert!(false, "{}", e);
        }
        Err(e) => {
          assert!(false, "{}", e);
        }
      };
    });

    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::FileStatusOkay);
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply in time!");
      }
    };

    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::ClosingDataConnection);
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply in time!");
      }
    };

    if let Err(e) = timeout(Duration::from_secs(3), dc_fut).await {
      panic!("Data channel future reached deadline!");
    };
  }

  #[tokio::test]
  async fn not_logged_in_test() {
    let ip = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");

    let command = Command::new(Commands::MLSD, String::new());

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));

    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(e) = timeout(
      Duration::from_secs(3),
      Mlsd::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    {
      panic!("Command timeout!");
    };

    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::NotLoggedIn);
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply in time!");
      }
    };
  }

  #[tokio::test]
  async fn not_directory_test() {
    let ip = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");

    let command = Command::new(Commands::MLSD, String::from("1MiB.txt"));

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));

    let permissions = HashSet::from([UserPermission::READ, UserPermission::LIST]);
    let root_path = std::env::current_dir().unwrap().join("test_files");
    let label = "test_files";
    let view = FileSystemView::new(root_path.clone(), label.clone(), permissions.clone());
    let mut user_data = UserData::new("test", "test");
    user_data.add_view(view);

    session_properties.write().await.login(user_data);
    session_properties
      .write()
      .await
      .file_system_view_root
      .change_working_directory(label.clone());

    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(e) = timeout(
      Duration::from_secs(3),
      Mlsd::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    {
      panic!("Command timeout!");
    };

    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::SyntaxErrorInParametersOrArguments);
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply in time!");
      }
    };
  }

  #[tokio::test]
  async fn nonexistent_test() {
    let ip = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");

    let command = Command::new(Commands::MLSD, String::from("NONEXISTENT"));

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));

    let permissions = HashSet::from([UserPermission::READ, UserPermission::LIST]);
    let root_path = std::env::current_dir().unwrap().join("test_files");
    let label = "test_files";
    let view = FileSystemView::new(root_path.clone(), label.clone(), permissions.clone());
    let mut user_data = UserData::new("test", "test");
    user_data.add_view(view);

    session_properties.write().await.login(user_data);

    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(e) = timeout(
      Duration::from_secs(3),
      Mlsd::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    {
      panic!("Command timeout!");
    };

    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::FileUnavailable);
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply in time!");
      }
    };
  }

  #[tokio::test]
  async fn insufficient_permissions_test() {
    let ip = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");

    let command = Command::new(Commands::MLSD, String::new());

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));

    let permissions = HashSet::from([]);
    let root_path = std::env::current_dir().unwrap().join("test_files");
    let label = "test_files";
    let view = FileSystemView::new(root_path.clone(), label.clone(), permissions.clone());
    let mut user_data = UserData::new("test", "test");
    user_data.add_view(view);

    session_properties.write().await.login(user_data);
    session_properties
      .write()
      .await
      .file_system_view_root
      .change_working_directory(label.clone());

    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(e) = timeout(
      Duration::from_secs(3),
      Mlsd::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    {
      panic!("Command timeout!");
    };

    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::FileUnavailable);
        assert!(result.to_string().contains("Insufficient permissions!"));
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply in time!");
      }
    };
  }

  #[tokio::test]
  async fn data_channel_not_open_tcp() {
    let ip = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");

    let command = Command::new(Commands::MLSD, String::new());

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));

    let permissions = HashSet::from([UserPermission::READ, UserPermission::LIST]);
    let root_path = std::env::current_dir().unwrap().join("test_files");
    let label = "test_files";
    let view = FileSystemView::new(root_path.clone(), label.clone(), permissions.clone());
    let mut user_data = UserData::new("test", "test");
    user_data.add_view(view);

    session_properties.write().await.login(user_data);

    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(e) = timeout(
      Duration::from_secs(3),
      Mlsd::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    {
      panic!("Command timeout!");
    };

    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::BadSequenceOfCommands);
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply in time!");
      }
    };
  }
}
