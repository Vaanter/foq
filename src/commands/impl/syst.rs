use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use std::sync::Arc;

pub(crate) async fn syst(command: &Command, reply_sender: Arc<impl ReplySend>) {
  debug_assert_eq!(command.command, Commands::Syst);
  reply_sender
    .send_control_message(Reply::new(ReplyCode::NameSystemType, "UNIX Type: L8"))
    .await;
}

#[cfg(test)]
mod tests {
  use std::env::current_dir;
  use std::sync::Arc;
  use std::time::Duration;

  use tokio::sync::mpsc;
  use tokio::time::timeout;

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::reply_code::ReplyCode;
  use crate::utils::test_utils::{
    receive_and_verify_reply, setup_test_command_processor_custom, CommandProcessorSettingsBuilder,
    TestReplySender,
  };

  #[tokio::test]
  async fn response_test() {
    let command = Command::new(Commands::Syst, "");

    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap().join("test_files"))
      .build()
      .expect("Settings should be valid");

    let command_processor = setup_test_command_processor_custom(&settings);
    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    if timeout(
      Duration::from_secs(2),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
      .is_err()
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(2, &mut rx, ReplyCode::NameSystemType, Some("UNIX")).await;
  }
}
