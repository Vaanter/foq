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

    if !command.argument.is_empty() {
      Pwd::reply(
        Reply::new(
          ReplyCode::SyntaxErrorInParametersOrArguments,
          "PWD must not have an argument!",
        ),
        reply_sender,
      )
      .await;
      return;
    }

    if !command_processor.session_properties.read().await.is_logged_in() {
      Pwd::reply(
        Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"),
        reply_sender,
      )
      .await;
      return;
    }

    let session_properties = command_processor.session_properties.read().await;
    let reply_message = format!("\"{}\"", session_properties
      .file_system_view_root
      .get_current_working_directory());
    Pwd::reply(
      Reply::new(ReplyCode::PathnameCreated, reply_message),
      reply_sender,
    )
    .await;
  }
}
