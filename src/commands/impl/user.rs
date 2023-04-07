use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::command_processor::CommandProcessor;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;

pub(crate) struct User;

#[async_trait]
impl Executable for User {
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::USER);

    if command.argument.is_empty() {
      User::reply(
        Reply::new(
          ReplyCode::SyntaxErrorInParametersOrArguments,
          "No username specified!",
        ),
        reply_sender,
      )
      .await;
      return;
    }

    let mut session_properties = command_processor.session_properties.write().await;
    let _ = session_properties
      .login_form
      .username
      .insert(command.argument.clone());

    User::reply(
      Reply::new(ReplyCode::UserNameOkay, "User name okay, need password."),
      reply_sender,
    )
    .await;
  }
}
