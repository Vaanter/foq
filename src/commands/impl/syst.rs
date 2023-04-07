use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;
use crate::io::command_processor::CommandProcessor;

pub(crate) struct Syst;

#[async_trait]
impl Executable for Syst {
  async fn execute(command_processor: &mut CommandProcessor, command: &Command, reply_sender: &mut impl ReplySend) {
    debug_assert_eq!(command.command, Commands::SYST);
    Syst::reply(
      Reply::new(ReplyCode::NameSystemType, "UNIX Type: L8"),
      reply_sender,
    )
    .await;
  }
}
