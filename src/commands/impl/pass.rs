use std::sync::Arc;
use tracing::{error, info};

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::global_context::AUTH_PROVIDER;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;

#[tracing::instrument(skip(command_processor, reply_sender))]
pub(crate) async fn pass(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  debug_assert_eq!(command.command, Commands::Pass);

  let password = command.argument.as_str();
  if password.is_empty() {
    return reply_sender
      .send_control_message(Reply::new(
        ReplyCode::SyntaxErrorInParametersOrArguments,
        "No password supplied",
      ))
      .await;
  }

  let session_properties = command_processor.session_properties.clone();

  if session_properties
    .read()
    .await
    .login_form
    .username
    .is_none()
  {
    return reply_sender
      .send_control_message(Reply::new(
        ReplyCode::BadSequenceOfCommands,
        "Supply the username first!",
      ))
      .await;
  }

  let provider = match AUTH_PROVIDER.get() {
    Some(provider) => provider,
    None => {
      error!("Database connection not setup!");
      return reply_sender
        .send_control_message(Reply::new(
          ReplyCode::ServiceNotAvailableClosingControlConnection,
          "Unknown error occurred!",
        ))
        .await;
    }
  };

  let mut form = session_properties.read().await.login_form.clone();
  let _ = form.password.insert(password.to_string());
  let username = form.username.as_ref().unwrap().clone();
  info!("User '{}' attempting login.", &username);
  let result = session_properties.write().await.login(provider, form).await;

  return if result {
    info!("User '{}' logged in successfully", &username);
    reply_sender
      .send_control_message(Reply::new(ReplyCode::UserLoggedIn, "Log in successful"))
      .await
  } else {
    info!("User '{}' failed to login!", &username);
    reply_sender
      .send_control_message(Reply::new(ReplyCode::NotLoggedIn, "Incorrect credentials!"))
      .await
  };
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;
  use std::time::Duration;

  use tokio::sync::mpsc::channel;
  use tokio::sync::RwLock;
  use tokio::time::timeout;

  use crate::auth::user_data::UserData;
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::reply_code::ReplyCode;
  use crate::data_channels::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::global_context::AUTH_PROVIDER;
  use crate::session::command_processor::CommandProcessor;
  use crate::session::session_properties::SessionProperties;
  use crate::utils::test_utils::{
    create_test_auth_provider, receive_and_verify_reply, TestReplySender, LOCALHOST,
  };

  #[tokio::test]
  async fn login_successful_test() {
    let mut session_properties = SessionProperties::new();
    let _ = session_properties
      .login_form
      .username
      .insert("test".to_string());

    let session_properties = Arc::new(RwLock::new(session_properties));
    let wrapper = Arc::new(StandardDataChannelWrapper::new(LOCALHOST));
    let command_processor = CommandProcessor::new(session_properties, wrapper);

    let command = Command::new(Commands::Pass, "test");

    let users = vec![UserData::new("test", "test")];
    AUTH_PROVIDER
      .get_or_init(|| async { create_test_auth_provider(users) })
      .await;

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::UserLoggedIn, None).await;
  }

  #[tokio::test]
  async fn incorrect_password_test() {
    let mut session_properties = SessionProperties::new();
    let _ = session_properties
      .login_form
      .username
      .insert("test".to_string());

    let session_properties = Arc::new(RwLock::new(session_properties));
    let wrapper = Arc::new(StandardDataChannelWrapper::new(LOCALHOST));
    let command_processor = CommandProcessor::new(session_properties, wrapper);

    let command = Command::new(Commands::Pass, "INVALID");

    let users = vec![UserData::new("test", "test")];
    AUTH_PROVIDER
      .get_or_init(|| async { create_test_auth_provider(users) })
      .await;

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::NotLoggedIn, None).await;
  }

  #[tokio::test]
  async fn no_username_test() {
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let wrapper = Arc::new(StandardDataChannelWrapper::new(LOCALHOST));
    let command_processor = CommandProcessor::new(session_properties, wrapper);

    let command = Command::new(Commands::Pass, "test");

    let users = vec![UserData::new("test", "test")];
    AUTH_PROVIDER
      .get_or_init(|| async { create_test_auth_provider(users) })
      .await;

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::BadSequenceOfCommands, None).await;
  }

  #[tokio::test]
  async fn no_password_test() {
    let mut session_properties = SessionProperties::new();
    let _ = session_properties
      .login_form
      .username
      .insert("test".to_string());

    let session_properties = Arc::new(RwLock::new(session_properties));
    let wrapper = Arc::new(StandardDataChannelWrapper::new(LOCALHOST));
    let command_processor = CommandProcessor::new(session_properties, wrapper);

    let command = Command::new(Commands::Pass, "");

    let users = vec![UserData::new("test", "test")];
    AUTH_PROVIDER
      .get_or_init(|| async { create_test_auth_provider(users) })
      .await;

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(
      2,
      &mut rx,
      ReplyCode::SyntaxErrorInParametersOrArguments,
      None,
    )
    .await;
  }

  #[tokio::test]
  #[ignore] // Requires other tests that initialize DB to not run
  async fn database_not_setup_test() {
    let mut session_properties = SessionProperties::new();
    let _ = session_properties
      .login_form
      .username
      .insert("test".to_string());

    let session_properties = Arc::new(RwLock::new(session_properties));
    let wrapper = Arc::new(StandardDataChannelWrapper::new(LOCALHOST));
    let command_processor = CommandProcessor::new(session_properties, wrapper);

    let command = Command::new(Commands::Pass, "test");

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(
      2,
      &mut rx,
      ReplyCode::ServiceNotAvailableClosingControlConnection,
      None,
    )
    .await;
  }
}
