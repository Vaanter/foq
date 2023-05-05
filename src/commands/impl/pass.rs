use async_trait::async_trait;
use tracing::{error, info};

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::global_context::AUTH_PROVIDER;
use crate::handlers::reply_sender::ReplySend;
use crate::io::command_processor::CommandProcessor;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;

pub(crate) struct Pass;

#[async_trait]
impl Executable for Pass {
  #[tracing::instrument(skip(command_processor, reply_sender))]
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
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

    let session_properties = command_processor.session_properties.clone();

    if session_properties
      .read()
      .await
      .login_form
      .username
      .is_none()
    {
      Self::reply(
        Reply::new(
          ReplyCode::BadSequenceOfCommands,
          "Supply the username first!",
        ),
        reply_sender,
      )
      .await;
      return;
    }

    let provider = match AUTH_PROVIDER.get() {
      Some(provider) => provider,
      None => {
        error!("Database connection not setup!");
        Self::reply(
          Reply::new(
            ReplyCode::ServiceNotAvailableClosingControlConnection,
            "Unknown error occurred!",
          ),
          reply_sender,
        )
        .await;
        return;
      }
    };

    let mut form = session_properties.read().await.login_form.clone();
    let _ = form.password.insert(password.to_string());
    let username = form.username.as_ref().unwrap().clone();
    info!("User '{}' attempting login.", &username);
    let result = session_properties.write().await.login(provider, form).await;

    if result {
      info!("User '{}' logged in successfully", &username);
      Self::reply(
        Reply::new(ReplyCode::UserLoggedIn, "Log in successful"),
        reply_sender,
      )
      .await;
      return;
    } else {
      info!("User '{}' failed to login!", &username);
      Self::reply(
        Reply::new(ReplyCode::NotLoggedIn, "Incorrect credentials!"),
        reply_sender,
      )
      .await;
      return;
    }
  }
}

#[cfg(test)]
mod tests {
  use std::net::SocketAddr;
  use std::sync::Arc;
  use std::time::Duration;

  use tokio::sync::mpsc::channel;
  use tokio::sync::{Mutex, RwLock};
  use tokio::time::timeout;

  use crate::auth::user_data::UserData;
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::pass::Pass;
  use crate::global_context::AUTH_PROVIDER;
  use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::command_processor::CommandProcessor;
  use crate::io::reply_code::ReplyCode;
  use crate::io::session_properties::SessionProperties;
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
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let command = Command::new(Commands::PASS, "test");

    let users = vec![UserData::new("test", "test")];
    AUTH_PROVIDER
      .get_or_init(|| async { create_test_auth_provider(users) })
      .await;

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      Pass::execute(&mut command_processor, &command, &mut reply_sender),
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
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let command = Command::new(Commands::PASS, "INVALID");

    let users = vec![UserData::new("test", "test")];
    AUTH_PROVIDER
      .get_or_init(|| async { create_test_auth_provider(users) })
      .await;

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      Pass::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::NotLoggedIn, None).await;
  }

  #[tokio::test]
  async fn no_username_test() {
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let command = Command::new(Commands::PASS, "test");

    let users = vec![UserData::new("test", "test")];
    AUTH_PROVIDER
      .get_or_init(|| async { create_test_auth_provider(users) })
      .await;

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      Pass::execute(&mut command_processor, &command, &mut reply_sender),
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
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let command = Command::new(Commands::PASS, "");

    let users = vec![UserData::new("test", "test")];
    AUTH_PROVIDER
      .get_or_init(|| async { create_test_auth_provider(users) })
      .await;

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      Pass::execute(&mut command_processor, &command, &mut reply_sender),
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
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let command = Command::new(Commands::PASS, "test");

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      Pass::execute(&mut command_processor, &command, &mut reply_sender),
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
