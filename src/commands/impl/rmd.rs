use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::r#impl::shared::get_delete_reply;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;
use std::sync::Arc;
use tracing::info;

#[tracing::instrument(skip(command_processor, reply_sender))]
pub(crate) async fn rmd(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  assert_eq!(Commands::Rmd, command.command);

  let session_properties = command_processor.session_properties.read().await;

  if !session_properties.is_logged_in() {
    return reply_sender
      .send_control_message(Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"))
      .await;
  }

  info!(
    "User '{}' deleting directory '{}'.",
    session_properties.username.as_ref().unwrap(),
    &command.argument
  );

  let result = session_properties
    .file_system_view_root
    .delete_folder(&command.argument)
    .await;

  reply_sender
    .send_control_message(get_delete_reply(result, true))
    .await;
}

#[cfg(test)]
mod tests {
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::reply_code::ReplyCode;
  use crate::utils::test_utils::*;
  use std::env::temp_dir;
  use std::sync::Arc;
  use std::time::Duration;
  use tokio::sync::mpsc;
  use tokio::time::timeout;
  use uuid::Uuid;

  #[tokio::test]
  async fn rmd_absolute_test() {
    let label = "test";
    let root = temp_dir();
    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root.join(&dir_name);
    std::fs::create_dir(&dir_path).expect("Creating test directory should succeed");
    let _cleanup = DirCleanup::new(&dir_path);
    let command = Command::new(Commands::Rmd, format!("/{}/{}", label, dir_name));

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.to_string())
      .view_root(root)
      .username(Some("test_user".to_string()))
      .build()
      .unwrap();
    let command_processor = setup_test_command_processor_custom(&settings);

    assert!(dir_path.exists());
    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::RequestedFileActionOkay, None).await;
    assert!(!dir_path.exists());
  }

  #[tokio::test]
  async fn rmd_relative_with_label_test() {
    let label = "test";
    let root = temp_dir();
    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root.join(&dir_name);
    std::fs::create_dir(&dir_path).expect("Creating test directory should succeed");
    let _cleanup = DirCleanup::new(&dir_path);
    let command = Command::new(Commands::Rmd, format!("{}/{}", label, dir_name));

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.to_string())
      .view_root(root)
      .username(Some("test_user".to_string()))
      .build()
      .unwrap();
    let command_processor = setup_test_command_processor_custom(&settings);

    assert!(dir_path.exists());
    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::RequestedFileActionOkay, None).await;
    assert!(!dir_path.exists());
  }

  #[tokio::test]
  async fn rmd_relative_test() {
    let label = "test";
    let root = temp_dir();
    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root.join(&dir_name);
    std::fs::create_dir(&dir_path).expect("Creating test directory should succeed");
    let _cleanup = DirCleanup::new(&dir_path);
    let command = Command::new(Commands::Rmd, &dir_name);

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.to_string())
      .view_root(root)
      .username(Some("test_user".to_string()))
      .change_path(Some(label.to_string()))
      .build()
      .unwrap();
    let command_processor = setup_test_command_processor_custom(&settings);

    assert!(dir_path.exists());
    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::RequestedFileActionOkay, None).await;
    assert!(!dir_path.exists());
  }

  #[tokio::test]
  async fn rmd_not_logged_in_test() {
    let label = "test";
    let root = temp_dir();
    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root.join(&dir_name);
    std::fs::create_dir(&dir_path).expect("Creating test directory should succeed");
    let _cleanup = DirCleanup::new(&dir_path);
    let command = Command::new(Commands::Rmd, format!("/{}/{}", label, dir_name));

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.to_string())
      .view_root(root)
      .build()
      .unwrap();
    let command_processor = setup_test_command_processor_custom(&settings);

    assert!(dir_path.exists());
    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::NotLoggedIn, None).await;
    assert!(dir_path.exists());
  }

  #[tokio::test]
  async fn rmd_file_test() {
    let label = "test";
    let root = temp_dir();
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);
    let command = Command::new(Commands::Rmd, &file_name);

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.to_string())
      .view_root(root)
      .username(Some("test_user".to_string()))
      .change_path(Some(label.to_string()))
      .build()
      .unwrap();
    let command_processor = setup_test_command_processor_custom(&settings);

    assert!(file_path.exists());
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
      ReplyCode::SyntaxErrorInParametersOrArguments,
      None,
    )
    .await;
    assert!(file_path.exists());
  }
}
