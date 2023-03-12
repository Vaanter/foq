use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;
use crate::io::session::Session;

pub(crate) struct Pass;

#[async_trait]
impl Executable for Pass {
  async fn execute(session: &mut Session, command: &Command, reply_sender: &mut impl ReplySend) {
    debug_assert_eq!(command.command, Commands::PASS);

    let password = command.argument.as_str();
    if password.is_empty() {
      Pass::reply(
        Reply::new(
          ReplyCode::SyntaxErrorInParametersOrArguments,
          "No password supplied",
        ),
        reply_sender,
      )
      .await;
      return;
    }

    Pass::reply(
      Reply::new(ReplyCode::UserLoggedIn, "Logged in"),
      reply_sender,
    )
    .await;
  }
}
