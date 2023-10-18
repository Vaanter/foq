use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::commands::r#impl::shared::get_change_directory_reply;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;

#[derive(Copy, Clone, Eq, PartialEq, Default)]
pub struct Cdup;

#[async_trait]
impl Executable for Cdup {
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::Cdup);

    let mut session_properties = command_processor.session_properties.write().await;

    if !session_properties.is_logged_in() {
      Self::reply(
        Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"),
        reply_sender,
      )
      .await;
      return;
    }

    if !command.argument.is_empty() {
      return Self::reply(
        Reply::new(
          ReplyCode::SyntaxErrorInParametersOrArguments,
          "CDUP must not have an argument!",
        ),
        reply_sender,
      )
      .await;
    }

    let result = session_properties
      .file_system_view_root
      .change_working_directory_up();
    let reply = get_change_directory_reply(result);

    Self::reply(reply, reply_sender).await;
  }
}

#[cfg(test)]
mod tests {
  use std::env::current_dir;
  use std::path::PathBuf;
  use std::time::Duration;

  use tokio::sync::mpsc;
  use tokio::time::timeout;

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::cdup::Cdup;
  use crate::commands::reply_code::ReplyCode;
  use crate::utils::test_utils::{
    setup_test_command_processor_custom, CommandProcessorSettings, CommandProcessorSettingsBuilder,
    TestReplySender,
  };

  async fn common(
    settings: &CommandProcessorSettings,
    command: Command,
    reply_code: ReplyCode,
    expected_path: PathBuf,
    expected_display_path: &str,
  ) {
    let mut command_processor = setup_test_command_processor_custom(&settings);
    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    Cdup::execute(&mut command_processor, &command, &mut reply_sender).await;
    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, reply_code);
        if reply_code != ReplyCode::NotLoggedIn {
          let root = &command_processor
            .session_properties
            .read()
            .await
            .file_system_view_root;
          let view = root
            .file_system_views
            .as_ref()
            .unwrap()
            .get(&settings.label.clone());
          assert!(view.is_some());
          assert_eq!(view.unwrap().current_path, expected_path);
          assert_eq!(root.get_current_working_directory(), expected_display_path);
        }
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply!");
      }
    };
  }

  #[tokio::test]
  async fn cdup_with_argument_should_reply_501() {
    let path = current_dir().unwrap();
    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .username(Some("testuser".to_string()))
      .view_root(path.clone())
      .build()
      .expect("Settings should be valid");

    let command = Command::new(Commands::Cdup, "path");

    common(
      &settings,
      command,
      ReplyCode::SyntaxErrorInParametersOrArguments,
      path.clone().canonicalize().unwrap(),
      "/",
    )
    .await;
  }

  #[tokio::test]
  async fn cdup_from_root_should_reply_550() {
    let path = current_dir().unwrap();
    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .username(Some("testuser".to_string()))
      .view_root(path.clone())
      .build()
      .expect("Settings should be valid");

    let command = Command::new(Commands::Cdup, "");

    common(
      &settings,
      command,
      ReplyCode::FileUnavailable,
      path.clone().canonicalize().unwrap(),
      "/",
    )
    .await;
  }

  #[tokio::test]
  async fn cdup_from_view_should_return_to_root_and_reply_250() {
    let path = current_dir().unwrap();
    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .username(Some("testuser".to_string()))
      .view_root(path.clone())
      .change_path(Some(label.clone()))
      .build()
      .expect("Settings should be valid");

    let command = Command::new(Commands::Cdup, "");

    common(
      &settings,
      command,
      ReplyCode::RequestedFileActionOkay,
      path.clone().canonicalize().unwrap(),
      "/",
    )
    .await;
  }

  #[tokio::test]
  async fn cdup_not_logged_in_should_reply_530() {
    let path = current_dir().unwrap();

    let settings = CommandProcessorSettingsBuilder::default()
      .build()
      .expect("Settings should be valid");

    let command = Command::new(Commands::Cdup, "");

    common(
      &settings,
      command,
      ReplyCode::NotLoggedIn,
      path.clone().canonicalize().unwrap(),
      "/",
    )
    .await;
  }
}
