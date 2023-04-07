use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;
use crate::io::command_processor::CommandProcessor;

pub(crate) struct Type;

#[async_trait]
impl Executable for Type {
  async fn execute(command_processor: &mut CommandProcessor, command: &Command, reply_sender: &mut impl ReplySend) {
    debug_assert_eq!(command.command, Commands::TYPE);

    Type::reply(
      Reply::new(ReplyCode::CommandOkay, "TYPE set to I"),
      reply_sender,
    )
    .await;
  }
}
