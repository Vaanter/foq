use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;

pub(crate) async fn pwd(
  command: &Command,
  command_processor: &mut CommandProcessor,
  reply_sender: &mut impl ReplySend,
) {
  debug_assert_eq!(command.command, Commands::Pwd);

  let session_properties = command_processor.session_properties.read().await;

  if !command.argument.is_empty() {
    return reply_sender
      .send_control_message(Reply::new(
        ReplyCode::SyntaxErrorInParametersOrArguments,
        "PWD must not have an argument!",
      ))
      .await;
  }

  if !session_properties.is_logged_in() {
    return reply_sender
      .send_control_message(Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"))
      .await;
  }

  let reply_message = format!(
    "\"{}\"",
    session_properties
      .file_system_view_root
      .get_current_working_directory()
  );
  reply_sender
    .send_control_message(Reply::new(ReplyCode::PathnameCreated, reply_message))
    .await;
}

#[cfg(test)]
mod tests {
  use std::env::current_dir;
  use std::time::Duration;

  use tokio::sync::mpsc::channel;
  use tokio::time::timeout;

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::reply_code::ReplyCode;
  use crate::utils::test_utils::{
    receive_and_verify_reply, setup_test_command_processor_custom, CommandProcessorSettingsBuilder,
    TestReplySender,
  };

  #[tokio::test]
  async fn with_argument_test() {
    let command = Command::new(Commands::Pwd, "/test_files");

    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap().join("test_files"))
      .build()
      .expect("Settings should be valid");

    let mut command_processor = setup_test_command_processor_custom(&settings);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(&mut command_processor, &mut reply_sender),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(
      2,
      &mut rx,
      ReplyCode::SyntaxErrorInParametersOrArguments,
      None,
    )
    .await;
  }

  #[tokio::test]
  async fn not_logged_in_test() {
    let command = Command::new(Commands::Pwd, "");

    let settings = CommandProcessorSettingsBuilder::default()
      .build()
      .expect("Settings should be valid");
    let mut command_processor = setup_test_command_processor_custom(&settings);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(&mut command_processor, &mut reply_sender),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::NotLoggedIn, None).await;
  }

  #[tokio::test]
  async fn format_test() {
    let command = Command::new(Commands::Pwd, "");

    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap().join("test_files"))
      .build()
      .expect("Settings should be valid");

    let mut command_processor = setup_test_command_processor_custom(&settings);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(&mut command_processor, &mut reply_sender),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::PathnameCreated, Some("/")).await;
  }
}
