use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;

#[derive(Copy, Clone, Eq, PartialEq, Default)]
pub(crate) struct Feat;

const FEATURES: [&str; 2] = ["MLSD", "UTF8"];

impl Feat {
  fn format_features() -> Vec<String> {
    FEATURES.iter().map(|f| format!(" {}", f)).collect()
  }
}

#[async_trait]
impl Executable for Feat {
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::FEAT);
    let mut lines: Vec<String> = vec!["Features supported: ".to_string()];
    lines.append(&mut Feat::format_features());
    lines.push("END".to_string());
    Feat::reply(
      Reply::new_multiline(ReplyCode::SystemStatus, lines),
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
  use crate::commands::r#impl::feat::Feat;
  use crate::data_channels::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::session::command_processor::CommandProcessor;
  use crate::session::session_properties::SessionProperties;
  use crate::utils::test_utils::{TestReplySender, LOCALHOST};

  #[tokio::test]
  async fn format() {
    const EXPECTED: &str = " MLSD UTF8";
    assert_eq!(EXPECTED, Feat::format_features().join(""))
  }

  #[tokio::test]
  async fn full_reply() {
    const EXPECTED: &str = "211-Features supported: \r\n MLSD\r\n UTF8\r\n211 END\r\n";
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let mut session = CommandProcessor::new(
      session_properties.clone(),
      Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST))),
    );
    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    Feat::execute(
      &mut session,
      &Command::new(Commands::FEAT, ""),
      &mut reply_sender,
    )
    .await;
    match rx.recv().await {
      Some(reply) => assert_eq!(EXPECTED, reply.to_string()),
      None => panic!("Rx closed without reading reply!"),
    }
  }
}
