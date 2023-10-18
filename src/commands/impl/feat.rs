use std::iter::Iterator;

use async_trait::async_trait;
use once_cell::sync::Lazy;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;

#[derive(Copy, Clone, Eq, PartialEq, Default)]
pub(crate) struct Feat;

static LINES: Lazy<Vec<String>> = Lazy::new(|| {
  let mut lines: Vec<String> = vec!["Supported features:".to_string()];
  let features = ["MLSD", "REST STREAM", "UTF8"];
  lines.extend(features.iter().map(|f| format!(" {}", f)));
  lines.push("END".to_string());
  lines
});

#[async_trait]
impl Executable for Feat {
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::Feat);

    Feat::reply(
      Reply::new_multiline(ReplyCode::SystemStatus, LINES.clone()),
      reply_sender,
    )
    .await;
  }
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use tokio::sync::mpsc::channel;
  use tokio::sync::{Mutex, RwLock};

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::feat::{Feat, LINES};
  use crate::data_channels::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::session::command_processor::CommandProcessor;
  use crate::session::session_properties::SessionProperties;
  use crate::utils::test_utils::{TestReplySender, LOCALHOST};

  #[tokio::test]
  async fn format_test() {
    assert_eq!(LINES.first().unwrap(), "Supported features:");
    assert_eq!(LINES.last().unwrap(), "END");
  }

  #[tokio::test]
  async fn full_reply_test() {
    const EXPECTED: &str =
      "211-Supported features:\r\n MLSD\r\n REST STREAM\r\n UTF8\r\n211 END\r\n";
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let mut session = CommandProcessor::new(
      session_properties.clone(),
      Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST))),
    );
    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    Feat::execute(
      &mut session,
      &Command::new(Commands::Feat, ""),
      &mut reply_sender,
    )
    .await;
    match rx.recv().await {
      Some(reply) => assert_eq!(EXPECTED, reply.to_string()),
      None => panic!("Rx closed without reading reply!"),
    }
  }
}
