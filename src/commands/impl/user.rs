use std::collections::BTreeMap;
use std::path::PathBuf;

use async_trait::async_trait;

use crate::auth::user_data::UserData;
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

    let acl = BTreeMap::from([(PathBuf::from("C:/"), true)]);

    // Test implementation!
    let user_data = UserData {
      username: username.to_string(),
      acl,
    };

    session.set_user(user_data);
    User::reply(
      Reply::new(
        ReplyCode::UserNameOkay,
        &format!("Password required for {}", username),
      ),
      reply_sender,
    )
    .await;
  }
}
