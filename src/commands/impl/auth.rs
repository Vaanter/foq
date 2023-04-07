use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;
use crate::io::command_processor::CommandProcessor;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub(crate) struct Auth;

#[async_trait]
impl Executable for Auth {
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::AUTH);
    Auth::reply(
      Reply::new(ReplyCode::AuthNotAvailable, "SSL/TLS not available"),
      reply_sender,
    )
      .await;
  }
}
