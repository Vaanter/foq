//! Denotes that a type can be executed. Must be implemented by all commands.

use async_trait::async_trait;

use crate::commands::command::Command;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;
use crate::commands::reply::Reply;

#[async_trait]
pub(crate) trait Executable {
  /// Executes a command.
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  );

  async fn reply(reply: Reply, reply_sender: &mut impl ReplySend) {
    reply_sender.send_control_message(reply).await;
  }
}
