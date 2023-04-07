use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::command_processor::CommandProcessor;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;

#[derive(Copy, Clone, Eq, PartialEq, Default)]
pub struct Cdup;

#[async_trait]
impl Executable for Cdup {
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::CDUP);

    let mut session_properties = command_processor.session_properties.write().await;

    if !session_properties.is_logged_in() {
      Cdup::reply(
        Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"),
        reply_sender,
      )
      .await;
      return;
    }

    let result = session_properties
      .file_system_view_root
      .change_working_directory_up();
    let reply = if result {
      Reply::new(ReplyCode::RequestedFileActionOkay, "Path changed!")
    } else {
      Reply::new(ReplyCode::RequestedFileActionNotTaken, "Path not changed!")
    };

    Cdup::reply(reply, reply_sender).await;
  }
}

#[cfg(test)]
mod tests {
  use std::collections::HashSet;
  use std::path::PathBuf;
  use std::sync::Arc;
  use std::time::Duration;

  use tokio::sync::{mpsc, Mutex, RwLock};
  use tokio::time::timeout;

  use crate::auth::user_data::UserData;
  use crate::auth::user_permission::UserPermission;
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::cdup::Cdup;
  use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::command_processor::CommandProcessor;
  use crate::io::file_system_view::FileSystemView;
  use crate::io::file_system_view_root::FileSystemViewRoot;
  use crate::io::reply_code::ReplyCode;
  use crate::io::reply_code::ReplyCode::{RequestedFileActionNotTaken, RequestedFileActionOkay};
  use crate::io::session_properties::SessionProperties;
  use crate::utils::test_utils::TestReplySender;

  async fn common(
    label: &str,
    root: PathBuf,
    change_path: &str,
    reply_code: ReplyCode,
    expected_path: PathBuf,
    expected_display_path: &str,
  ) {
    let command = Command::new(Commands::CDUP, "".to_string());
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let mut session = CommandProcessor::new(
      session_properties.clone(),
      Arc::new(Mutex::new(StandardDataChannelWrapper::new(
        "127.0.0.1:0".parse().unwrap(),
      ))),
    );
    let permissions = HashSet::from([UserPermission::READ]);
    let view = FileSystemView::new(root, label.clone(), permissions);
    let mut user_data = UserData::new("test", "test");
    user_data.add_view(view);
    let root = FileSystemViewRoot::default();
    session_properties.write().await.file_system_view_root = root;
    session_properties.write().await.login(user_data);
    session_properties
      .write()
      .await
      .file_system_view_root
      .change_working_directory(change_path);
    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    Cdup::execute(&mut session, &command, &mut reply_sender).await;
    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, reply_code);
        let root = &session_properties.read().await.file_system_view_root;
        let view = root.file_system_views.as_ref().unwrap().get(label);
        assert!(view.is_some());
        assert_eq!(view.unwrap().current_path, expected_path);
        assert_eq!(root.get_current_working_directory(), expected_display_path);
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply!");
      }
    };
  }

  #[tokio::test]
  async fn cdup_from_root_should_reply_450() {
    let path = PathBuf::from("/");
    common(
      "test",
      path.clone(),
      "",
      RequestedFileActionNotTaken,
      path.clone(),
      "/",
    )
    .await;
  }

  #[tokio::test]
  async fn cdup_from_view_should_return_to_root_and_reply_250() {
    let path = std::env::current_dir().unwrap();
    common(
      "test",
      path.clone(),
      "test",
      RequestedFileActionOkay,
      path.clone(),
      "/",
    )
    .await;
  }
}
