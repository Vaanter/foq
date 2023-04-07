use std::path::PathBuf;
use std::str::FromStr;

use async_trait::async_trait;
use tokio::fs::OpenOptions;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;
use crate::io::session::Session;

pub(crate) struct Retr;

#[async_trait]
impl Executable for Retr {
  async fn execute(command_processor: &mut CommandProcessor, command: &Command, reply_sender: &mut impl ReplySend) {
    debug_assert_eq!(command.command, Commands::RETR);

    if command.argument.is_empty() {
      Retr::reply(Reply::new(
          ReplyCode::SyntaxErrorInParametersOrArguments,
          "No file specified!",
        ), reply_sender)
        .await;
      return;
    }

    //TODO path handling

    let mut file_path = PathBuf::from_str(&command.argument).unwrap(); // Cannot fail

    // Check if file exists and server has permissions to access it
    if let Err(_) | Ok(false) = file_path.try_exists() {
      reply_sender
        .send_control_message(Reply::new(
          ReplyCode::FileUnavailable,
          "File does not exist or insufficient permissions!",
        ))
        .await;
      return;
    }

    if file_path.is_relative() {
      file_path = session.cwd.clone().join(file_path);
    }

    // Check if user is logged in and has access permissions
    match &session.user_data {
      Some(user_data) => {
        let acl = &user_data.acl;
        let access = acl.iter().any(|ac| {
          file_path
            .parent()
            .expect("File must have parent!")
            .starts_with(ac.0)
            && *ac.1
        });
        if !access {
          reply_sender
            .send_control_message(Reply::new(
              ReplyCode::FileUnavailable,
              "User has no permissions for this file",
            ))
            .await;
          return;
        }
      }
      None => {
        reply_sender
          .send_control_message(Reply::new(ReplyCode::NotLoggedIn, "Log in first!"))
          .await;
        return;
      }
    if let Err(_) | Ok(false) = file_path.try_exists() {
      Retr::reply(Reply::new(ReplyCode::FileUnavailable, "File does not exist!"), reply_sender).await;
      return;
    }

    let data_channel = command_processor
      .data_wrapper
      .clone()
      .lock()
      .await
      .get_data_stream()
      .await;
    let mut data_channel = match data_channel.try_lock() {
      Ok(dc) => {
        if dc.is_none() {
          Retr::reply(Reply::new(
              ReplyCode::BadSequenceOfCommands,
              "Data channel must be open first!",
            ), reply_sender)
            .await;
          return;
        }
        dc
      }
      Err(e) => {
        eprintln!("Data channel is not available! {e}");
        Retr::reply(Reply::new(
            ReplyCode::BadSequenceOfCommands,
            "Data channel must be open first!",
          ), reply_sender)
          .await;
        return;
      }
    };

    let mut file = match OpenOptions::new().read(true).open(file_path).await {
      Ok(file) => file,
      Err(_) => {
        Retr::reply(Reply::new(
            ReplyCode::RequestedFileActionNotTaken,
            "File inaccessible!",
          ), reply_sender)
          .await;
        return;
      }
    };

    Retr::reply(Reply::new(
        ReplyCode::FileStatusOkay,
        "Starting file transfer!",
      ), reply_sender)
      .await;

    let success = match tokio::io::copy(&mut file, data_channel.as_mut().unwrap()).await {
      Ok(len) => {
        println!("Sent {len} bytes.");
        true
      }
      Err(e) => {
        eprintln!("Error sending file!");
        false
      }
    };

    if success {
      Retr::reply(Reply::new(
          ReplyCode::ClosingDataConnection,
          "Transfer complete!",
        ), reply_sender)
        .await;
    } else {
      Retr::reply(Reply::new(
          ReplyCode::ConnectionClosedTransferAborted,
          "Error occurred during transfer!",
        ), reply_sender)
        .await;
    }

    // Needed to release the lock. Maybe find a better way to do this?
    drop(data_channel);
    command_processor.data_wrapper.lock().await.close_data_stream().await;
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
  use tokio::sync::Mutex;
  use tokio::time::timeout;

  use crate::auth::user_data::UserData;
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::retr::Retr;
  use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::reply_code::ReplyCode;
  use crate::io::command_processor::CommandProcessor;
  use crate::utils::test_utils::TestReplySender;

  async fn common(file_name: &'static str) {
    let ip: SocketAddr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");

    if !Path::new(&file_name).exists() {
      panic!("Test file does not exist! Cannot proceed!");
    }

    let command = Command::new(Commands::RETR, file_name);

    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let mut session = Session::new_with_defaults(wrapper);
    let user = UserData::new(
      "Test".to_string(),
      BTreeMap::from([(PathBuf::from("C:\\"), true)]),
    );
    session.set_user(user);
    let addr = match session
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

    let _ = session
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
        Retr::execute(&mut session, &command, &mut reply_sender),
      )
      .await
      {
        panic!("Command timeout!");
      };
    });

    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::FileStatusOkay);
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply!");
      }
    };

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
      assert_eq!(local_file_hasher.finalize(), sent_file_hasher.finalize(), "File hashes do not match!");
      println!("File hashes match!");
    };

    match timeout(Duration::from_secs(5), transfer).await {
      Ok(()) => println!("Transfer complete!"),
      Err(e) => panic!("Transfer timed out!"),
    }

    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::ClosingDataConnection);
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply!");
      }
    };

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
}
