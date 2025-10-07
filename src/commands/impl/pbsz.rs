use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;
use std::sync::Arc;

#[tracing::instrument(skip(command_processor, reply_sender))]
pub(crate) async fn pbsz(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  debug_assert_eq!(command.command, Commands::Pbsz);

  let buffer_size = &command.argument.parse::<u32>();
  let mut properties = command_processor.session_properties.write().await;

  match buffer_size {
    Ok(0) => {
      properties.pbsz = Some(0);
      reply_sender
        .send_control_message(Reply::new(ReplyCode::CommandOkay, "Buffer size set"))
        .await;
    }
    Ok(_) => {
      properties.pbsz = Some(0);
      reply_sender
        .send_control_message(Reply::new(
          ReplyCode::CommandOkay,
          "PBSZ=0, provided size is not supported!",
        ))
        .await;
    }
    Err(_) => {
      properties.pbsz = None;
      reply_sender
        .send_control_message(Reply::new(
          ReplyCode::SyntaxErrorInParametersOrArguments,
          "Invalid buffer size!",
        ))
        .await;
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::reply_code::ReplyCode;
  use crate::utils::test_utils::{
    CommandProcessorSettingsBuilder, TestReplySender, receive_and_verify_reply,
    setup_test_command_processor_custom,
  };
  use std::sync::Arc;
  use std::time::Duration;
  use tokio::sync::mpsc;
  use tokio::time::timeout;

  #[tokio::test]
  async fn zero_argument_test() {
    let command = Command::new(Commands::Pbsz, "0");

    let settings =
      CommandProcessorSettingsBuilder::default().build().expect("Settings should be valid");

    let command_processor = setup_test_command_processor_custom(&settings);
    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    if timeout(
      Duration::from_secs(2),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .is_err()
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(2, &mut rx, ReplyCode::CommandOkay, None).await;
  }

  #[tokio::test]
  async fn non_zero_argument_test() {
    let command = Command::new(Commands::Pbsz, "10");

    let settings =
      CommandProcessorSettingsBuilder::default().build().expect("Settings should be valid");

    let command_processor = setup_test_command_processor_custom(&settings);
    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    if timeout(
      Duration::from_secs(2),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .is_err()
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(2, &mut rx, ReplyCode::CommandOkay, Some("PBSZ=0")).await;
  }

  #[tokio::test]
  async fn non_integer_argument_test() {
    let command = Command::new(Commands::Pbsz, "value");

    let settings =
      CommandProcessorSettingsBuilder::default().build().expect("Settings should be valid");

    let command_processor = setup_test_command_processor_custom(&settings);
    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    if timeout(
      Duration::from_secs(2),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .is_err()
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(2, &mut rx, ReplyCode::SyntaxErrorInParametersOrArguments, None).await;
  }

  #[tokio::test]
  async fn empty_argument_test() {
    let command = Command::new(Commands::Pbsz, "");

    let settings =
      CommandProcessorSettingsBuilder::default().build().expect("Settings should be valid");

    let command_processor = setup_test_command_processor_custom(&settings);
    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    if timeout(
      Duration::from_secs(2),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .is_err()
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(2, &mut rx, ReplyCode::SyntaxErrorInParametersOrArguments, None).await;
  }
}
