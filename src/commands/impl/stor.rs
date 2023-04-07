use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::command_processor::CommandProcessor;

pub(crate) struct Stor;

#[async_trait]
impl Executable for Stor {
  async fn execute(command_processor: &mut CommandProcessor, command: &Command, reply_sender: &mut impl ReplySend) {
    debug_assert_eq!(command.command, Commands::STOR);

    

    todo!()
  }
}
