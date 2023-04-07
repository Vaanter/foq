use async_trait::async_trait;

use crate::commands::command::Command;
use crate::handlers::reply_sender::ReplySend;
use crate::io::reply::Reply;
use crate::io::command_processor::CommandProcessor;

#[async_trait]
pub(crate) trait Executable {
  async fn execute(command_processor: &mut CommandProcessor, command: &Command, reply_sender: &mut impl ReplySend);

  async fn reply(reply: Reply, reply_sender: &mut impl ReplySend) {
    reply_sender.send_control_message(reply).await;
  }
}
