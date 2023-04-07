use std::path::PathBuf;
use std::str::FromStr;

use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::command_processor::CommandProcessor;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;

pub(crate) struct Pwd;

#[async_trait]
impl Executable for Pwd {
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::PWD);
    if command.argument.is_empty() {
      Pwd::reply(
        Reply::new(ReplyCode::CommandOkay, session.cwd.to_str().unwrap()),
        reply_sender,
      )
      .await;
      return;
    }

    let new_path = PathBuf::from_str(&command.argument).unwrap();
    let reply = match session.set_path(new_path) {
      true => Reply::new(ReplyCode::CommandOkay, session.cwd.to_str().unwrap()),
      false => Reply::new(ReplyCode::FileUnavailable, ""),
    };
    Pwd::reply(reply, reply_sender).await;
  }
}
