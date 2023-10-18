use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::commands::r#impl::shared::get_change_directory_reply;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;

#[derive(Copy, Clone, Eq, PartialEq, Default)]
pub(crate) struct Cwd;

#[async_trait]
impl Executable for Cwd {
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::Cwd);

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

    let result = session_properties
      .file_system_view_root
      .change_working_directory(new_path);
    let reply = get_change_directory_reply(result);

    Self::reply(reply, reply_sender).await;
  }
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use tokio::sync::mpsc;
  use tokio::time::timeout;

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::cwd::Cwd;
  use crate::commands::reply_code::ReplyCode;
  use crate::utils::test_utils::{
    receive_and_verify_reply, setup_test_command_processor, setup_test_command_processor_custom,
    CommandProcessorSettingsBuilder, TestReplySender,
  };

  #[tokio::test]
  async fn cwd_absolute_test() {
    let command = Command::new(Commands::Cwd, "/test");

    let (_, mut command_processor) = setup_test_command_processor();

    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      Cwd::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::RequestedFileActionOkay, None).await;
    assert_eq!(
      command_processor
        .session_properties
        .read()
        .await
        .file_system_view_root
        .get_current_working_directory(),
      "/test"
    );
  }

  #[tokio::test]
  async fn to_current_test() {
    let command = Command::new(Commands::Cwd, "/");

    let (_, mut command_processor) = setup_test_command_processor();

    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      Cwd::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::RequestedFileActionOkay, None).await;
    assert_eq!(
      command_processor
        .session_properties
        .read()
        .await
        .file_system_view_root
        .get_current_working_directory(),
      "/"
    );
  }

  #[tokio::test]
  async fn not_logged_in_test() {
    let command = Command::new(Commands::Cwd, "/test");

    let settings = CommandProcessorSettingsBuilder::default()
      .build()
      .expect("Settings should be valid");

    let mut command_processor = setup_test_command_processor_custom(&settings);

    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      Cwd::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::NotLoggedIn, None).await;
    assert_eq!(
      command_processor
        .session_properties
        .read()
        .await
        .file_system_view_root
        .get_current_working_directory(),
      "/"
    );
  }

  #[tokio::test]
  async fn no_argument_test() {
    let command = Command::new(Commands::Cwd, "");

    let (_, mut command_processor) = setup_test_command_processor();

    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      Cwd::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(
      2,
      &mut rx,
      ReplyCode::SyntaxErrorInParametersOrArguments,
      None,
    )
    .await;
    assert_eq!(
      command_processor
        .session_properties
        .read()
        .await
        .file_system_view_root
        .get_current_working_directory(),
      "/"
    );
  }
}
