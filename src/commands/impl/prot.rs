use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::global_context::TLS_ACCEPTOR;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;
use crate::session::protection_mode::ProtMode;
use std::str::FromStr;
use std::sync::Arc;

#[tracing::instrument(skip(command_processor, reply_sender))]
pub(crate) async fn prot(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  debug_assert_eq!(command.command, Commands::Prot);

  let mut properties = command_processor.session_properties.write().await;

  let reply = match ProtMode::from_str(&command.argument) {
    Ok(ProtMode::Clear) => {
      properties.prot_mode = ProtMode::Clear;
      Reply::new(ReplyCode::CommandOkay, "Protection level set")
    }
    Ok(ProtMode::Safe) | Ok(ProtMode::Confidential) => Reply::new(
      ReplyCode::ProtectionLevelNotSupported,
      "Protection mode not available",
    ),
    Ok(ProtMode::Private) => {
      if TLS_ACCEPTOR.clone().is_some() {
        properties.prot_mode = ProtMode::Private;
        Reply::new(ReplyCode::CommandOkay, "Protection level set")
      } else {
        Reply::new(ReplyCode::AuthNotAvailable, "Protection not available")
      }
    }
    Err(_) => Reply::new(
      ReplyCode::CommandNotImplementedForThatParameter,
      "Unknown protection mode",
    ),
  };

  reply_sender.send_control_message(reply).await;
}
