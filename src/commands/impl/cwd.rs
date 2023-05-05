use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::command_processor::CommandProcessor;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;

#[derive(Copy, Clone, Eq, PartialEq, Default)]
pub(crate) struct Cwd;

#[async_trait]
impl Executable for Cwd {
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::CWD);

    let mut session_properties = command_processor.session_properties.write().await;

    if !session_properties.is_logged_in() {
      Self::reply(
        Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"),
        reply_sender,
      )
      .await;
      return;
    }

    let new_path = &command.argument;
    if new_path.is_empty() {
      Self::reply(
        Reply::new(
          ReplyCode::SyntaxErrorInParametersOrArguments,
          "No path specified!",
        ),
        reply_sender,
      )
      .await;
      return;
    }

    if session_properties
      .file_system_view_root
      .change_working_directory(new_path)
    {
      Self::reply(
        Reply::new(ReplyCode::RequestedFileActionOkay, "Path changed."),
        reply_sender,
      )
      .await;
      return;
    } else {
      Self::reply(
        Reply::new(ReplyCode::RequestedFileActionOkay, "Path not changed!"),
        reply_sender,
      )
      .await;
      return;
    }
  }
}

#[cfg(test)]
mod tests {
  use std::collections::HashSet;
  use std::env::current_dir;
  use std::sync::Arc;
  use std::time::Duration;

  use tokio::sync::{mpsc, Mutex, RwLock};
  use tokio::time::timeout;

  use crate::auth::user_permission::UserPermission;
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::cwd::Cwd;
  use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::command_processor::CommandProcessor;
  use crate::io::file_system_view::FileSystemView;
  use crate::io::reply_code::ReplyCode;
  use crate::io::session_properties::SessionProperties;
  use crate::utils::test_utils::{receive_and_verify_reply, TestReplySender, LOCALHOST};

  #[tokio::test]
  async fn cwd_absolute_test() {
    let command = Command::new(Commands::CWD, "/test");

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

    let session_properties = Arc::new(RwLock::new(session_properties));
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let mut command_processor = CommandProcessor::new(session_properties.clone(), wrapper);

    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(_) = timeout(
      Duration::from_secs(3),
      Cwd::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(2, &mut rx, ReplyCode::RequestedFileActionOkay, None).await;
    assert_eq!(
      session_properties
        .read()
        .await
        .file_system_view_root
        .get_current_working_directory(),
      "/test"
    );
  }
