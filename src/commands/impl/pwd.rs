use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::command_processor::CommandProcessor;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;

pub(crate) struct Pwd;

#[async_trait]
impl Executable for Pwd {
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::PWD);

    if !command.argument.is_empty() {
      Self::reply(
        Reply::new(
          ReplyCode::SyntaxErrorInParametersOrArguments,
          "PWD must not have an argument!",
        ),
        reply_sender,
      )
      .await;
      return;
    }

    if !command_processor
      .session_properties
      .read()
      .await
      .is_logged_in()
    {
      Self::reply(
        Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"),
        reply_sender,
      )
      .await;
      return;
    }

    let session_properties = command_processor.session_properties.read().await;
    let reply_message = format!(
      "\"{}\"",
      session_properties
        .file_system_view_root
        .get_current_working_directory()
    );
    Self::reply(
      Reply::new(ReplyCode::PathnameCreated, reply_message),
      reply_sender,
    )
    .await;
  }
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;
  use std::time::Duration;

  use tokio::sync::mpsc::channel;
  use tokio::sync::{Mutex, RwLock};
  use tokio::time::timeout;

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::pwd::Pwd;
  use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::command_processor::CommandProcessor;
  use crate::io::reply_code::ReplyCode;
  use crate::io::session_properties::SessionProperties;
  use crate::utils::test_utils::{receive_and_verify_reply, TestReplySender, LOCALHOST};

  #[tokio::test]
  async fn with_argument_test() {
    let command = Command::new(Commands::PWD, "/test_files");

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      Pwd::execute(&mut command_processor, &command, &mut reply_sender),
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
  }

  #[tokio::test]
  async fn not_logged_in_test() {
    let command = Command::new(Commands::PWD, "");

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      Pwd::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::NotLoggedIn, None).await;
  }

  #[tokio::test]
  async fn format_test() {
    let command = Command::new(Commands::PWD, "");

    let mut session_properties = SessionProperties::new();
    let _ = session_properties.username.insert("test".to_string());

    let session_properties = Arc::new(RwLock::new(session_properties));
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      Pwd::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::PathnameCreated, Some("/")).await;
  }
}
