use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::r#impl::shared::get_change_directory_reply;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;
use std::sync::Arc;

pub(crate) async fn cwd(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  debug_assert_eq!(command.command, Commands::Cwd);

  let mut session_properties = command_processor.session_properties.write().await;

  if !session_properties.is_logged_in() {
    return reply_sender
      .send_control_message(Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"))
      .await;
  }

  let new_path = &command.argument;
  if new_path.is_empty() {
    return reply_sender
      .send_control_message(Reply::new(
        ReplyCode::SyntaxErrorInParametersOrArguments,
        "No path specified!",
      ))
      .await;
  }

  let result = session_properties
    .file_system_view_root
    .change_working_directory(new_path);
  let reply = get_change_directory_reply(result);

  reply_sender.send_control_message(reply).await;
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;
  use std::time::Duration;

  use tokio::sync::mpsc;
  use tokio::time::timeout;

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::reply_code::ReplyCode;
  use crate::utils::test_utils::{
    receive_and_verify_reply, setup_test_command_processor, setup_test_command_processor_custom,
    CommandProcessorSettingsBuilder, TestReplySender,
  };

  #[tokio::test]
  async fn cwd_absolute_test() {
    let command = Command::new(Commands::Cwd, "/test");

    let (_, command_processor) = setup_test_command_processor();
    let command_processor = Arc::new(command_processor);

    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::RequestedFileActionOkay, None).await;
    assert_eq!(
      command_processor
        .session_properties
        .read()
        .await
        .file_system_view_root
        .get_current_working_directory(),
      "/test"
    );
  }

  #[tokio::test]
  async fn to_current_test() {
    let command = Command::new(Commands::Cwd, "/");

    let (_, command_processor) = setup_test_command_processor();
    let command_processor = Arc::new(command_processor);

    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::RequestedFileActionOkay, None).await;
    assert_eq!(
      command_processor
        .session_properties
        .read()
        .await
        .file_system_view_root
        .get_current_working_directory(),
      "/"
    );
  }

  #[tokio::test]
  async fn not_logged_in_test() {
    let command = Command::new(Commands::Cwd, "/test");

    let settings = CommandProcessorSettingsBuilder::default()
      .build()
      .expect("Settings should be valid");

    let command_processor = Arc::new(setup_test_command_processor_custom(&settings));

    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::NotLoggedIn, None).await;
    assert_eq!(
      command_processor
        .session_properties
        .read()
        .await
        .file_system_view_root
        .get_current_working_directory(),
      "/"
    );
  }

  #[tokio::test]
  async fn no_argument_test() {
    let command = Command::new(Commands::Cwd, "");

    let (_, command_processor) = setup_test_command_processor();
    let command_processor = Arc::new(command_processor);

    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
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
    assert_eq!(
      command_processor
        .session_properties
        .read()
        .await
        .file_system_view_root
        .get_current_working_directory(),
      "/"
    );
  }
}
