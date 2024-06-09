use std::sync::Arc;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;

#[tracing::instrument(skip(command_processor, reply_sender))]
pub(crate) async fn opts(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  debug_assert_eq!(Commands::Opts, command.command);
  let mut session_properties = command_processor.session_properties.write().await;

  if !session_properties.is_logged_in() {
    reply_sender
      .send_control_message(Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"))
      .await;
    return;
  }

  if command.argument.is_empty() {
    return reply_sender
      .send_control_message(Reply::new(
        ReplyCode::SyntaxErrorInParametersOrArguments,
        "OPTS must have an argument!",
      ))
      .await;
  }

  return match command.argument.to_uppercase().as_str() {
    "UTF8 ON" => {
      session_properties.utf8 = true;
      reply_sender
        .send_control_message(Reply::new(
          ReplyCode::CommandOkay,
          "UTF8 is always enabled.",
        ))
        .await
    }
    _ => {
      reply_sender
        .send_control_message(Reply::new(
          ReplyCode::SyntaxErrorInParametersOrArguments,
          "OPTS parameter not recognized!",
        ))
        .await
    }
  };
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use tokio::sync::mpsc::channel;
  use tokio::time::timeout;

  use crate::utils::test_utils::{
    receive_and_verify_reply, setup_test_command_processor, TestReplySender,
  };

  use super::*;

  #[tokio::test]
  pub async fn enable_utf8_test() {
    let (_, command_processor) = setup_test_command_processor();
    let command_processor = Arc::new(command_processor);

    let command = Command::new(Commands::Opts, "UTF8 ON");

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::CommandOkay, None).await;
  }
}
