use once_cell::sync::Lazy;
use std::iter::Iterator;
use std::sync::Arc;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;

static LINES: Lazy<Vec<String>> = Lazy::new(|| {
  let mut lines: Vec<String> = vec!["Supported features:".to_string()];
  #[cfg(not(windows))]
  let features = ["MLSD", "MFMT", "REST STREAM", "UTF8", "RMDA <path>"];
  #[cfg(windows)]
  let features = ["MLSD", "MFMT", "MFCT", "REST STREAM", "UTF8", "RMDA <path>"];
  lines.extend(features.iter().map(|f| format!(" {}", f)));
  lines.push("END".to_string());
  lines
});

#[tracing::instrument(skip(reply_sender))]
pub(crate) async fn feat(command: &Command, reply_sender: Arc<impl ReplySend>) {
  debug_assert_eq!(command.command, Commands::Feat);
  reply_sender
    .send_control_message(Reply::new_multiline(ReplyCode::SystemStatus, LINES.clone()))
    .await;
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;
  use tokio::sync::mpsc::channel;

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::r#impl::feat::{LINES, feat};
  use crate::utils::test_utils::TestReplySender;

  #[tokio::test]
  async fn format_test() {
    assert_eq!(LINES.first().unwrap(), "Supported features:");
    assert_eq!(LINES.last().unwrap(), "END");
  }

  #[tokio::test]
  async fn full_reply_test() {
    #[cfg(not(windows))]
    const EXPECTED: &str = "211-Supported features:\r\n MLSD\r\n MFMT\r\n REST STREAM\r\n UTF8\r\n RMDA <path>\r\n211 END\r\n";
    #[cfg(windows)]
    const EXPECTED: &str = "211-Supported features:\r\n MLSD\r\n MFMT\r\n MFCT\r\n REST STREAM\r\n UTF8\r\n RMDA <path>\r\n211 END\r\n";
    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    let command = Command::new(Commands::Feat, "");
    feat(&command, Arc::new(reply_sender)).await;
    match rx.recv().await {
      Some(reply) => assert_eq!(EXPECTED, reply.to_string()),
      None => panic!("Rx closed without reading reply!"),
    }
  }
}
