use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;
use std::sync::Arc;

#[tracing::instrument(skip(command_processor, reply_sender))]
pub(crate) async fn rest(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  debug_assert_eq!(command.command, Commands::Rest);

  if command.argument.is_empty() {
    reply_sender
      .send_control_message(Reply::new(
        ReplyCode::SyntaxErrorInParametersOrArguments,
        "Offset not specified!",
      ))
      .await;
    return;
  }

  let mut session_properties = command_processor.session_properties.write().await;

  if !session_properties.is_logged_in() {
    return reply_sender
      .send_control_message(Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"))
      .await;
  }

  let offset = match command.argument.parse::<u64>() {
    Ok(off) => off,
    Err(_) => {
      return reply_sender
        .send_control_message(Reply::new(
          ReplyCode::SyntaxErrorInParametersOrArguments,
          "Unable to parse offset!",
        ))
        .await;
    }
  };

  session_properties.offset = offset;

  reply_sender
    .send_control_message(Reply::new(
      ReplyCode::RequestedFileActionPendingFurtherInformation,
      format!("Restarting at {}", session_properties.offset),
    ))
    .await;
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;
  use std::time::Duration;

  use tokio::sync::mpsc::channel;
  use tokio::time::timeout;

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::reply_code::ReplyCode;
  use crate::utils::test_utils::{
    receive_and_verify_reply, setup_test_command_processor, TestReplySender,
  };

  #[tokio::test]
  async fn set_test() {
    let (_, command_processor) = setup_test_command_processor();
    let command_processor = Arc::new(command_processor);

    let command = Command::new(Commands::Rest, "123");

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
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
    assert_eq!(
      123,
      command_processor.session_properties.read().await.offset
    );
  }

  #[tokio::test]
  async fn set_not_logged_in_test() {
    let (_, command_processor) = setup_test_command_processor();
    let command_processor = Arc::new(command_processor);

    command_processor.session_properties.write().await.username = None;

    let command = Command::new(Commands::Rest, "123");

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::NotLoggedIn, None).await;
    assert_eq!(0, command_processor.session_properties.read().await.offset);
  }

  #[tokio::test]
  async fn set_no_argument_test() {
    let (_, command_processor) = setup_test_command_processor();
    let command_processor = Arc::new(command_processor);

    let command = Command::new(Commands::Rest, String::new());

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
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
    let (_, command_processor) = setup_test_command_processor();
    let command_processor = Arc::new(command_processor);

    let command = Command::new(Commands::Rest, "test");

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
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
