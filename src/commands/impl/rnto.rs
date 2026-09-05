use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::r#impl::shared::get_rename_reply;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;
use std::sync::Arc;

pub(crate) async fn rnto(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  debug_assert_eq!(command.command, Commands::Rnto);

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
        "Rename target not specified!",
      ))
      .await;
    return;
  }

  let Some(rnfr) = session_properties.rnfr.take() else {
    reply_sender
      .send_control_message(Reply::new(
        ReplyCode::BadSequenceOfCommands,
        "Rename source not specified!",
      ))
      .await;
    return;
  };

  let rename_result =
    session_properties.file_system_view_root.rename_object(&rnfr, &command.argument).await;

  reply_sender.send_control_message(get_rename_reply(rename_result)).await
}

#[cfg(test)]
mod tests {
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::reply_code::ReplyCode;
  use crate::utils::test_utils::{
    CommandProcessorSettingsBuilder, FileCleanup, TestReplySender, receive_and_verify_reply,
    setup_test_command_processor_custom, setup_tracing, touch,
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
    let command = Command::new(Commands::Rnto, format!("/{}/{}", label, file_name));

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
    let command = Command::new(Commands::Rnto, "");

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
  async fn non_existent_source_test() {
    setup_tracing();
    let label = "test";
    let root = temp_dir();
    let file_name_target = "nonexistent-target.test".to_string();
    let command = Command::new(Commands::Rnto, file_name_target.clone());

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.to_string())
      .view_root(root)
      .username(Some("test_user".to_string()))
      .build()
      .unwrap();
    let command_processor = setup_test_command_processor_custom(&settings);
    command_processor.session_properties.write().await.rnfr =
      Some("nonexistent-source.test".to_string());

    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(
      2,
      &mut rx,
      ReplyCode::RequestedActionNotTakenFileNameNotAllowed,
      None,
    )
    .await;
  }

  #[tokio::test]
  async fn rename_to_view_root_fails_test() {
    setup_tracing();
    let label = "test";
    let root = temp_dir();
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root.join(&file_name);
    touch(&file_path).expect("Test file must exist");
    let _cleanup = FileCleanup::new(&file_path);
    let command = Command::new(Commands::Rnto, format!("/{}", Uuid::new_v4().as_hyphenated()));

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.to_string())
      .view_root(root)
      .username(Some("test_user".to_string()))
      .build()
      .unwrap();
    let command_processor = setup_test_command_processor_custom(&settings);
    command_processor.session_properties.write().await.rnfr =
      Some(format!("/{}/{}", label, file_name));

    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(
      2,
      &mut rx,
      ReplyCode::RequestedActionNotTakenFileNameNotAllowed,
      None,
    )
    .await;
  }

  #[tokio::test]
  async fn rename_succeeds_test() {
    setup_tracing();
    let label = "test";
    let root = temp_dir();
    let file_name_from = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_name_to = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path_from = root.join(&file_name_from);
    let file_path_to = root.join(&file_name_to);
    touch(&file_path_from).expect("Test file must exist");
    assert!(!file_path_to.exists());
    let _cleanup_from = FileCleanup::new(&file_path_from);
    let _cleanup_to = FileCleanup::new(&file_path_to);
    let command = Command::new(Commands::Rnto, format!("/{}/{}", label, file_name_to));

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.to_string())
      .view_root(root)
      .username(Some("test_user".to_string()))
      .build()
      .unwrap();
    let command_processor = setup_test_command_processor_custom(&settings);
    command_processor.session_properties.write().await.rnfr =
      Some(format!("/{}/{}", label, file_name_from));

    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::RequestedFileActionOkay, None).await;
    assert!(!file_path_from.exists());
    assert!(file_path_to.exists());
  }
}
