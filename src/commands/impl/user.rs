use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;
use std::sync::Arc;

#[tracing::instrument(skip(command_processor, reply_sender))]
pub(crate) async fn user(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  debug_assert_eq!(command.command, Commands::User);

  if command.argument.is_empty() {
    reply_sender
      .send_control_message(Reply::new(
        ReplyCode::SyntaxErrorInParametersOrArguments,
        "No username specified!",
      ))
      .await;
    return;
  }

  let mut session_properties = command_processor.session_properties.write().await;
  session_properties.login_form.username.replace(command.argument.clone());

  reply_sender
    .send_control_message(Reply::new(ReplyCode::UserNameOkay, "User name okay, need password."))
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
    CommandProcessorSettingsBuilder, TestReplySender, receive_and_verify_reply,
    setup_test_command_processor_custom,
  };

  #[tokio::test]
  async fn set_username_test() {
    let name = String::from("test");
    let command = Command::new(Commands::User, name.clone());

    let settings =
      CommandProcessorSettingsBuilder::default().build().expect("Settings should be valid");
    let command_processor = Arc::new(setup_test_command_processor_custom(&settings));

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::UserNameOkay, None).await;
    assert_eq!(command_processor.session_properties.read().await.login_form.username, Some(name));
  }

  #[tokio::test]
  async fn empty_username_test() {
    let command = Command::new(Commands::User, "");

    let settings =
      CommandProcessorSettingsBuilder::default().build().expect("Settings should be valid");
    let command_processor = setup_test_command_processor_custom(&settings);

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::SyntaxErrorInParametersOrArguments, None).await;
  }
}
