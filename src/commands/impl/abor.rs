use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;
use std::sync::Arc;
use tracing::debug;

#[tracing::instrument(skip(command_processor, reply_sender))]
pub(crate) async fn abor(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  debug_assert_eq!(Commands::Abor, command.command);

  let session_properties = command_processor.session_properties.read().await;

  if !session_properties.is_logged_in() {
    return reply_sender
      .send_control_message(Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"))
      .await;
  }

  if !command.argument.is_empty() {
    return reply_sender
      .send_control_message(Reply::new(
        ReplyCode::SyntaxErrorInParametersOrArguments,
        "ABOR must not have an argument!",
      ))
      .await;
  }

  debug!("Locking data channel");
  let data_channel_wrapper = command_processor.data_wrapper.lock().await;
  debug!("Aborting data channel");
  data_channel_wrapper.abort();

  debug!("Checking ABOR result");
  if data_channel_wrapper.get_data_stream().0.try_lock().is_ok() {
    reply_sender
      .send_control_message(Reply::new(
        ReplyCode::ClosingDataConnection,
        "Closing data connection.",
      ))
      .await;
  }
}
