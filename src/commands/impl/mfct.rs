use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::r#impl::shared::{get_modify_time_reply, parse_change_time};
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;
use std::fs::FileTimes;
use std::os::windows::fs::FileTimesExt;
use std::sync::Arc;

#[tracing::instrument(skip(command_processor, reply_sender))]
pub(crate) async fn mfct(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  debug_assert_eq!(command.command, Commands::Mfct);

  let session_properties = command_processor.session_properties.read().await;

  if !session_properties.is_logged_in() {
    reply_sender
      .send_control_message(Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"))
      .await;
    return;
  }

  let (timeval, path) = match parse_change_time(&command.argument) {
    Ok(parsed) => parsed,
    Err(r) => return reply_sender.send_control_message(r).await,
  };

  let result = session_properties
    .file_system_view_root
    .change_file_times(FileTimes::new().set_created(timeval.into()), path)
    .await;

  reply_sender
    .send_control_message(get_modify_time_reply(result, &timeval, path))
    .await;
}

#[cfg(test)]
mod tests {
  use crate::io::timeval::format_timeval;
  use crate::utils::test_utils::{
    receive_and_verify_reply, setup_test_command_processor, setup_test_command_processor_custom,
    touch, CommandProcessorSettingsBuilder, FileCleanup, TestReplySender,
  };
  use chrono::{DateTime, Local, TimeDelta, Timelike};
  use std::env::temp_dir;
  use std::fs::File;
  use std::ops::Sub;
  use std::time::Duration;
  use tokio::sync::mpsc;
  use tokio::time::timeout;
  use uuid::Uuid;

  use super::*;

  #[tokio::test]
  async fn mfct_not_logged_in_test() {
    let command = Command::new(Commands::Mfct, "20020717210715 file");
    let settings = CommandProcessorSettingsBuilder::default().build().unwrap();
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
  async fn mfct_invalid_timeval_test() {
    let command = Command::new(Commands::Mfct, "INVALID file");
    let (_, command_processor) = setup_test_command_processor();

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
  }

  #[tokio::test]
  async fn mfct_absolute_test() {
    let label = "test";
    let root = temp_dir();
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root.join(&file_name);
    touch(&file_path).expect("Test file must exist");
    let _cleanup = FileCleanup::new(&file_path);
    let timeval = Local::now()
      .sub(TimeDelta::hours(4))
      .with_nanosecond(0u32)
      .unwrap();
    let command = Command::new(
      Commands::Mfct,
      format!("{} /{}/{}", format_timeval(&timeval), label, file_name),
    );

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.to_string())
      .view_root(root)
      .username(Some("test_user".to_string()))
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

    receive_and_verify_reply(2, &mut rx, ReplyCode::FileStatus, None).await;
    let modification_time: DateTime<Local> = File::open(&file_path)
      .unwrap()
      .metadata()
      .unwrap()
      .created()
      .unwrap()
      .into();
    assert_eq!(timeval, modification_time);
  }

  #[tokio::test]
  async fn mfct_relative_with_label_test() {
    let label = "test";
    let root = temp_dir();
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root.join(&file_name);
    touch(&file_path).expect("Test file must exist");
    let _cleanup = FileCleanup::new(&file_path);
    let timeval = Local::now()
      .sub(TimeDelta::hours(4))
      .with_nanosecond(0u32)
      .unwrap();
    let command = Command::new(
      Commands::Mfct,
      format!("{} {}/{}", format_timeval(&timeval), label, file_name),
    );

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.to_string())
      .view_root(root)
      .username(Some("test_user".to_string()))
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

    receive_and_verify_reply(2, &mut rx, ReplyCode::FileStatus, None).await;
    let modification_time: DateTime<Local> = File::open(&file_path)
      .unwrap()
      .metadata()
      .unwrap()
      .created()
      .unwrap()
      .into();
    assert_eq!(timeval, modification_time);
  }

  #[tokio::test]
  async fn mfct_relative_test() {
    let label = "test";
    let root = temp_dir();
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root.join(&file_name);
    touch(&file_path).expect("Test file must exist");
    let _cleanup = FileCleanup::new(&file_path);
    let timeval = Local::now()
      .sub(TimeDelta::hours(4))
      .with_nanosecond(0u32)
      .unwrap();
    let command = Command::new(
      Commands::Mfct,
      format!("{} {}", format_timeval(&timeval), &file_name),
    );

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

    receive_and_verify_reply(2, &mut rx, ReplyCode::FileStatus, None).await;
    let modification_time: DateTime<Local> = File::open(&file_path)
      .unwrap()
      .metadata()
      .unwrap()
      .created()
      .unwrap()
      .into();
    assert_eq!(timeval, modification_time);
  }
}
