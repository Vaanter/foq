use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;
use crate::io::session::Session;

#[derive(Copy, Clone, Eq, PartialEq, Default)]
pub struct Cdup;

#[async_trait]
impl Executable for Cdup {
  async fn execute(session: &mut Session, command: &Command, reply_sender: &mut impl ReplySend) {
    debug_assert_eq!(command.command, Commands::CDUP);
    let new_path = session.cwd.parent();

    let reply = if new_path.is_some()
      && session.set_path(new_path.expect("Path should exist!").to_path_buf())
    {
      Reply::new(ReplyCode::RequestedFileActionOkay, "OK")
    } else {
      Reply::new(ReplyCode::RequestedFileActionNotTaken, "BAD")
    };

    Cdup::reply(reply, reply_sender).await;
  }
}

#[cfg(test)]
mod tests {
  use std::collections::BTreeMap;
  use std::path::PathBuf;
  use std::sync::Arc;
  use std::time::Duration;

  use tokio::sync::mpsc::channel;
  use tokio::sync::{mpsc, Mutex};
  use tokio::time::timeout;

  use crate::auth::user_data::UserData;
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::cdup::Cdup;
  use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::reply_code::ReplyCode;
  use crate::io::session::Session;
  use crate::utils::test_utils::TestReplySender;

  #[tokio::test]
  async fn given_windows_parent_accessible_and_valid_then_reply_250() {
    let command = Command::new(Commands::CDUP, "".to_string());
    let user = UserData::new(
      "Test".to_string(),
      BTreeMap::from([(PathBuf::from("C:\\"), true)]),
    );
    let mut session = Session::new_with_defaults(Arc::new(Mutex::new(
      StandardDataChannelWrapper::new("127.0.0.1:0".parse().unwrap()),
    )));
    session.user_data = Some(user);
    session.set_path(PathBuf::from("C:\\Users"));
    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    Cdup::execute(&mut session, &command, &mut reply_sender).await;
    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::RequestedFileActionOkay);
        assert_eq!(session.cwd, PathBuf::from("C:\\"));
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply!");
      }
    };
  }

  #[tokio::test]
  async fn given_windows_parent_accessible_but_invalid_then_reply_450() {
    let command = Command::new(Commands::CDUP, "".to_string());
    let user = UserData::new(
      "Test".to_string(),
      BTreeMap::from([(PathBuf::from("C:\\"), true)]),
    );
    let mut session = Session::new_with_defaults(Arc::new(Mutex::new(
      StandardDataChannelWrapper::new("127.0.0.1:0".parse().unwrap()),
    )));
    session.user_data = Some(user);
    session.set_path(PathBuf::from("C:\\"));
    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    Cdup::execute(&mut session, &command, &mut reply_sender).await;
    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::RequestedFileActionNotTaken);
        assert_eq!(session.cwd, PathBuf::from("C:\\"));
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply!");
      }
    };
  }

  #[tokio::test]
  async fn given_windows_parent_inaccessible_but_valid_then_reply_450() {
    let command = Command::new(Commands::CDUP, "".to_string());
    let user = UserData::new(
      "Test".to_string(),
      BTreeMap::from([(PathBuf::from("C:\\Users"), true)]),
    );
    let mut session = Session::new_with_defaults(Arc::new(Mutex::new(
      StandardDataChannelWrapper::new("127.0.0.1:0".parse().unwrap()),
    )));
    session.user_data = Some(user);
    session.set_path(PathBuf::from("C:\\Users"));
    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    Cdup::execute(&mut session, &command, &mut reply_sender).await;
    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::RequestedFileActionNotTaken);
        assert_eq!(session.cwd, PathBuf::from("C:\\Users"));
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply!");
      }
    };
  }

  #[tokio::test]
  async fn given_linux_parent_accessible_and_valid_then_reply_250() {
    let command = Command::new(Commands::CDUP, "".to_string());
    let user = UserData::new(
      "Test".to_string(),
      BTreeMap::from([(PathBuf::from("/"), true)]),
    );
    let mut session = Session::new_with_defaults(Arc::new(Mutex::new(
      StandardDataChannelWrapper::new("127.0.0.1:0".parse().unwrap()),
    )));
    session.user_data = Some(user);
    session.set_path(PathBuf::from("/home"));
    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    Cdup::execute(&mut session, &command, &mut reply_sender).await;
    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::RequestedFileActionOkay);
        assert_eq!(session.cwd, PathBuf::from("/"));
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply!");
      }
    };
  }

  #[tokio::test]
  async fn given_linux_parent_accessible_but_invalid_then_reply_450() {
    let command = Command::new(Commands::CDUP, "".to_string());
    let user = UserData::new(
      "Test".to_string(),
      BTreeMap::from([(PathBuf::from("/"), true)]),
    );
    let mut session = Session::new_with_defaults(Arc::new(Mutex::new(
      StandardDataChannelWrapper::new("127.0.0.1:0".parse().unwrap()),
    )));
    session.user_data = Some(user);
    session.set_path(PathBuf::from("/"));
    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    Cdup::execute(&mut session, &command, &mut reply_sender).await;
    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::RequestedFileActionNotTaken);
        assert_eq!(session.cwd, PathBuf::from("/"));
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply!");
      }
    };
  }

  #[tokio::test]
  async fn given_linux_parent_inaccessible_but_valid_then_reply_450() {
    let command = Command::new(Commands::CDUP, "".to_string());
    let user = UserData::new(
      "Test".to_string(),
      BTreeMap::from([(PathBuf::from("/home"), true)]),
    );
    let mut session = Session::new_with_defaults(Arc::new(Mutex::new(
      StandardDataChannelWrapper::new("127.0.0.1:0".parse().unwrap()),
    )));
    session.user_data = Some(user);
    session.set_path(PathBuf::from("/home"));
    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    Cdup::execute(&mut session, &command, &mut reply_sender).await;
    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::RequestedFileActionNotTaken);
        assert_eq!(session.cwd, PathBuf::from("/home"));
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply!");
      }
    };
  }
}
