use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;
use std::sync::Arc;

pub(crate) async fn rnfr(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  debug_assert_eq!(command.command, Commands::Rnfr);

  let mut session_properties = command_processor.session_properties.write().await;

  if !session_properties.is_logged_in() {
    reply_sender
      .send_control_message(Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"))
      .await;
    return;
  }

  if command.argument.is_empty() {
    reply_sender
      .send_control_message(Reply::new(
        ReplyCode::SyntaxErrorInParametersOrArguments,
        "Rename source not specified!",
      ))
      .await;
    return;
  }

  session_properties.rnfr.replace(command.argument.clone());

  reply_sender
    .send_control_message(Reply::new(
      ReplyCode::RequestedFileActionPendingFurtherInformation,
      "Rename source recorded",
    ))
    .await;
}

#[cfg(test)]
mod tests {
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::reply_code::ReplyCode;
  use crate::utils::test_utils::{
    CommandProcessorSettingsBuilder, TestReplySender, receive_and_verify_reply,
    setup_test_command_processor_custom, setup_tracing,
  };
  use std::env::temp_dir;
  use std::sync::Arc;
  use std::time::Duration;
  use tokio::sync::mpsc;
  use tokio::time::timeout;
  use uuid::Uuid;

  #[tokio::test]
  async fn not_logged_in_test() {
    setup_tracing();
    let label = "test";
    let root = temp_dir();
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let command = Command::new(Commands::Rnfr, format!("/{}/{}", label, file_name));

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.to_string())
      .view_root(root)
      .build()
      .unwrap();
    let command_processor = setup_test_command_processor_custom(&settings);

    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::NotLoggedIn, None).await;
  }

  #[tokio::test]
  async fn empty_path_test() {
    setup_tracing();
    let label = "test";
    let root = temp_dir();
    let command = Command::new(Commands::Rnfr, "");

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.to_string())
      .view_root(root)
      .username(Some("test_user".to_string()))
      .build()
      .unwrap();
    let command_processor = setup_test_command_processor_custom(&settings);

    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::SyntaxErrorInParametersOrArguments, None).await;
  }

  #[tokio::test]
  async fn valid_test() {
    setup_tracing();
    let label = "test";
    let root = temp_dir();
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let command = Command::new(Commands::Rnfr, file_name.clone());

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.to_string())
      .view_root(root)
      .username(Some("test_user".to_string()))
      .build()
      .unwrap();
    let command_processor = Arc::new(setup_test_command_processor_custom(&settings));

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
      ReplyCode::RequestedFileActionPendingFurtherInformation,
      None,
    )
    .await;

    assert_eq!(command_processor.session_properties.read().await.rnfr, Some(file_name));
  }
}
