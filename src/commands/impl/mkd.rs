use async_trait::async_trait;
use tracing::{info, trace};

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::commands::r#impl::shared::get_create_directory_reply;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;

#[derive(Copy, Clone, Eq, PartialEq, Default)]
pub(crate) struct Mkd;

#[async_trait]
impl Executable for Mkd {
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    trace!("Executing MKD command");
    debug_assert_eq!(command.command, Commands::Mkd);
    let session_properties = command_processor.session_properties.read().await;

    if !session_properties.is_logged_in() {
      Self::reply(
        Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"),
        reply_sender,
      )
      .await;
      return;
    }

    if command.argument.is_empty() {
      Self::reply(
        Reply::new(
          ReplyCode::SyntaxErrorInParametersOrArguments,
          "MKD must have an argument!",
        ),
        reply_sender,
      )
      .await;
      return;
    }

    info!("Creating directory");
    let result = session_properties
      .file_system_view_root
      .create_directory(&command.argument);

    Self::reply(get_create_directory_reply(result), reply_sender).await;
  }
}

#[cfg(test)]
mod tests {
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::mkd::Mkd;
  use crate::commands::reply_code::ReplyCode;
  use crate::utils::test_utils::{
    receive_and_verify_reply, setup_test_command_processor_custom, CommandProcessorSettingsBuilder,
    DirCleanup, TestReplySender,
  };
  use std::env::temp_dir;
  use std::time::Duration;
  use tokio::sync::mpsc::channel;
  use tokio::time::timeout;
  use uuid::Uuid;

  #[tokio::test]
  async fn not_logged_in_test() {
    let command = Command::new(Commands::Mkd, "");

    let settings = CommandProcessorSettingsBuilder::default()
      .build()
      .expect("Settings should be valid");
    let mut command_processor = setup_test_command_processor_custom(&settings);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      Mkd::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::NotLoggedIn, None).await;
  }

  #[tokio::test]
  async fn mkd_successful_test() {
    let new_dir_name = Uuid::new_v4();
    println!("Dir name: {}", new_dir_name);

    let label = "test".to_string();
    let virtual_path = format!("/{}/{}", label, new_dir_name);
    let command = Command::new(Commands::Mkd, &virtual_path);

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .username(Some("testuser".to_string()))
      .view_root(temp_dir())
      .build()
      .expect("Settings should be valid");

    let mut command_processor = setup_test_command_processor_custom(&settings);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    let dir_path = temp_dir().join(new_dir_name.to_string());
    let _d = DirCleanup::new(&dir_path);
    timeout(
      Duration::from_secs(3),
      Mkd::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::PathnameCreated, Some(&virtual_path)).await;
    assert!(dir_path.exists());
  }
}
