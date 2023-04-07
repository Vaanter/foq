use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;
use crate::io::command_processor::CommandProcessor;

pub(crate) struct Feat;

const FEATURES: [&str; 1] = ["MLSD"];

impl Feat {
  fn format_features() -> Vec<String> {
    FEATURES.iter().map(|f| format!(" {}", f)).collect()
  }
}

#[async_trait]
impl Executable for Feat {
  async fn execute(command_processor: &mut CommandProcessor, command: &Command, reply_sender: &mut impl ReplySend) {
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
  use tokio::sync::Mutex;
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::feat::Feat;
  use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::command_processor::CommandProcessor;
  use crate::utils::test_utils::TestReplySender;

  #[tokio::test]
  async fn format() {
    const EXPECTED: &str = " MLSD";
    assert_eq!(EXPECTED, Feat::format_features().join(""))
  }

  #[tokio::test]
  async fn full_reply() {
    const EXPECTED: &str = "211-Features supported: \r\n MLSD\r\n211 END\r\n";
    let mut session = Session::new_with_defaults(Arc::new(Mutex::new(
      StandardDataChannelWrapper::new("127.0.0.1:0".parse().unwrap()),
    )));
    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    Feat::execute(&mut session, &Command::new(Commands::FEAT, ""), &mut reply_sender).await;
    match rx.recv().await {
      Some(reply) => assert_eq!(EXPECTED, reply.to_string()),
      None => panic!("Rx closed without reading reply!"),
    }
  }
}
