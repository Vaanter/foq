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

#[cfg(test)]
mod tests {
  use std::sync::Arc;
  use std::time::Duration;
  use tokio::sync::{Mutex, RwLock};
  use tokio::sync::mpsc::channel;
  use tokio::time::timeout;
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::user::User;
  use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::command_processor::CommandProcessor;
  use crate::io::reply_code::ReplyCode;
  use crate::io::session_properties::SessionProperties;
  use crate::utils::test_utils::TestReplySender;

  #[tokio::test]
  async fn set_username_test() {
    let ip = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");

    let name = String::from("test");
    let command = Command::new(Commands::USER, name.clone());

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));

    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let mut command_processor = CommandProcessor::new(session_properties.clone(), wrapper);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(e) = timeout(
      Duration::from_secs(3),
      User::execute(&mut command_processor, &command, &mut reply_sender),
    )
      .await
    {
      panic!("Command timeout!");
    };

    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::UserNameOkay);
        assert_eq!(session_properties.read().await.login_form.username, Some(name));
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply in time!");
      }
    };
  }
}
