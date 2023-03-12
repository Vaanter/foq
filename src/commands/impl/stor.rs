use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::session::Session;

pub(crate) struct Stor;

#[async_trait]
impl Executable for Stor {
  async fn execute(session: &mut Session, command: &Command, reply_sender: &mut impl ReplySend) {
    debug_assert_eq!(command.command, Commands::STOR);

    

    todo!()
  }
}
