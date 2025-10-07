use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use std::sync::Arc;

#[tracing::instrument(skip(reply_sender))]
pub(crate) async fn noop(command: &Command, reply_sender: Arc<impl ReplySend>) {
  debug_assert_eq!(Commands::Noop, command.command);
  reply_sender.send_control_message(Reply::new(ReplyCode::CommandOkay, "OK")).await;
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;
  use std::time::Duration;

  use tokio::sync::RwLock;
  use tokio::sync::mpsc::channel;
  use tokio::time::timeout;

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::reply_code::ReplyCode;
  use crate::data_channels::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::session::command_processor::CommandProcessor;
  use crate::session::session_properties::SessionProperties;
  use crate::utils::test_utils::{LOCALHOST, TestReplySender, receive_and_verify_reply};

  #[tokio::test]
  async fn response_test() {
    let command = Command::new(Commands::Noop, "");

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));

    let wrapper = Arc::new(StandardDataChannelWrapper::new(LOCALHOST));
    let command_processor = CommandProcessor::new(session_properties.clone(), wrapper);

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    if timeout(
      Duration::from_secs(3),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .is_err()
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(2, &mut rx, ReplyCode::CommandOkay, Some("OK")).await;
  }
}
