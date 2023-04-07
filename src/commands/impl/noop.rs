use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;
use crate::io::command_processor::CommandProcessor;

pub(crate) struct Noop;

#[async_trait]
impl Executable for Noop {
  async fn execute(command_processor: &mut CommandProcessor, command: &Command, reply_sender: &mut impl ReplySend) {
    reply_sender
      .send_control_message(Reply::new(ReplyCode::CommandOkay, "OK"))
      .await;
  }
}
