use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;

#[derive(Copy, Clone, Eq, PartialEq, Default)]
pub(crate) struct Rest;

#[async_trait]
impl Executable for Rest {
  #[tracing::instrument(skip(command_processor, reply_sender))]
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::REST);

    if command.argument.is_empty() {
      Self::reply(
        Reply::new(
          ReplyCode::SyntaxErrorInParametersOrArguments,
          "Offset not specified!",
        ),
        reply_sender,
      )
      .await;
      return;
    }

    let mut session_properties = command_processor.session_properties.write().await;

    if !session_properties.is_logged_in() {
      return Self::reply(
        Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"),
        reply_sender,
      )
      .await;
    }

    let offset = match command.argument.parse::<u64>() {
      Ok(off) => off,
      Err(_) => {
        return Self::reply(
          Reply::new(
            ReplyCode::SyntaxErrorInParametersOrArguments,
            "Unable to parse offset!",
          ),
          reply_sender,
        )
        .await;
      }
    };

    session_properties.offset = offset;

    Self::reply(
      Reply::new(
        ReplyCode::RequestedFileActionPendingFurtherInformation,
        "Offset set",
      ),
      reply_sender,
    )
    .await;
  }
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use tokio::sync::mpsc::channel;
  use tokio::time::timeout;

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::rest::Rest;
  use crate::commands::reply_code::ReplyCode;
  use crate::utils::test_utils::{
    receive_and_verify_reply, setup_test_command_processor, TestReplySender,
  };

  #[tokio::test]
  async fn set_test() {
    let (_, mut command_processor) = setup_test_command_processor();

    let command = Command::new(Commands::REST, "123");

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      Rest::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(
      2,
      &mut rx,
      ReplyCode::RequestedFileActionPendingFurtherInformation,
      None,
    )
    .await;
    assert_eq!(123, command_processor.session_properties.read().await.offset);
  }

  #[tokio::test]
  async fn set_not_logged_in_test() {
    let (_, mut command_processor) = setup_test_command_processor();
    command_processor.session_properties.write().await.username = None;

    let command = Command::new(Commands::REST, "123");

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      Rest::execute(&mut command_processor, &command, &mut reply_sender),
    )
      .await
      .expect("Command timeout!");

    receive_and_verify_reply(
      2,
      &mut rx,
      ReplyCode::NotLoggedIn,
      None,
    )
      .await;
    assert_eq!(0, command_processor.session_properties.read().await.offset);
  }

  #[tokio::test]
  async fn set_no_argument_test() {
    let (_, mut command_processor) = setup_test_command_processor();

    let command = Command::new(Commands::REST, String::new());

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      Rest::execute(&mut command_processor, &command, &mut reply_sender),
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
    assert_eq!(0, command_processor.session_properties.read().await.offset);
  }

  #[tokio::test]
  async fn set_not_a_number_test() {
    let (_, mut command_processor) = setup_test_command_processor();

    let command = Command::new(Commands::REST, "test");

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      Rest::execute(&mut command_processor, &command, &mut reply_sender),
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
    assert_eq!(0, command_processor.session_properties.read().await.offset);
  }
}
