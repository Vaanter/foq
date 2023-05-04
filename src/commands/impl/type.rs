use async_trait::async_trait;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::command_processor::CommandProcessor;
use crate::io::data_type::{DataType, SubType};
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;

pub(crate) struct Type;

#[async_trait]
impl Executable for Type {
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::TYPE);

    if command.argument.is_empty() {
      Self::reply(
        Reply::new(
          ReplyCode::SyntaxErrorInParametersOrArguments,
          "Mode not specified!",
        ),
        reply_sender,
      )
      .await;
      return;
    }

    let (new_type, sub_type) = command
      .argument
      .split_once(" ")
      .unwrap_or((&command.argument, ""));

    match (new_type, sub_type) {
      ("A", "N") | ("A", "") => {
        command_processor.session_properties.write().await.data_type = DataType::ASCII {
          sub_type: SubType::NonPrint,
        }
      }
      ("A", "T") => {
        command_processor.session_properties.write().await.data_type = DataType::ASCII {
          sub_type: SubType::TelnetFormatEffectors,
        }
      }
      ("A", "C") => {
        command_processor.session_properties.write().await.data_type = DataType::ASCII {
          sub_type: SubType::CarriageControl,
        }
      }
      ("I", _) => command_processor.session_properties.write().await.data_type = DataType::BINARY,
      (_, _) => {
        Self::reply(
          Reply::new(
            ReplyCode::CommandNotImplementedForThatParameter,
            "Invalid or not supported mode!",
          ),
          reply_sender,
        )
        .await;
        return;
      }
    };

    Self::reply(
      Reply::new(ReplyCode::CommandOkay, format!("TYPE set to {}.", new_type)),
      reply_sender,
    )
    .await;
  }
}

#[cfg(test)]
mod tests {
  use std::net::SocketAddr;
  use std::sync::Arc;
  use std::time::Duration;

  use tokio::sync::{mpsc, Mutex, RwLock};
  use tokio::time::timeout;

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::r#type::Type;
  use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::command_processor::CommandProcessor;
  use crate::io::data_type::{DataType, SubType};
  use crate::io::reply_code::ReplyCode;
  use crate::io::session_properties::SessionProperties;
  use crate::utils::test_utils::{receive_and_verify_reply, TestReplySender};

  #[tokio::test]
  async fn ascii_non_print_test() {
    let ip: SocketAddr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let mut command_processor = CommandProcessor::new(session_properties.clone(), wrapper);

    let command = Command::new(Commands::TYPE, "A N");

    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(_) = timeout(
      Duration::from_secs(3),
      Type::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(2, &mut rx, ReplyCode::CommandOkay, None).await;
    assert_eq!(
      session_properties.read().await.data_type,
      DataType::ASCII {
        sub_type: SubType::NonPrint
      }
    );
  }

  #[tokio::test]
  async fn ascii_no_subtype_test() {
    let ip: SocketAddr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let mut command_processor = CommandProcessor::new(session_properties.clone(), wrapper);

    let command = Command::new(Commands::TYPE, "A");

    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(_) = timeout(
      Duration::from_secs(3),
      Type::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(2, &mut rx, ReplyCode::CommandOkay, None).await;
    assert_eq!(
      session_properties.read().await.data_type,
      DataType::ASCII {
        sub_type: SubType::NonPrint
      }
    );
  }

  #[tokio::test]
  async fn binary_test() {
    let ip: SocketAddr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let mut command_processor = CommandProcessor::new(session_properties.clone(), wrapper);

    let command = Command::new(Commands::TYPE, "I");

    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(_) = timeout(
      Duration::from_secs(3),
      Type::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(2, &mut rx, ReplyCode::CommandOkay, None).await;
    assert_eq!(session_properties.read().await.data_type, DataType::BINARY);
  }

  #[tokio::test]
  async fn ascii_tfe_test() {
    let ip: SocketAddr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let mut command_processor = CommandProcessor::new(session_properties.clone(), wrapper);

    let command = Command::new(Commands::TYPE, "A T");

    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(_) = timeout(
      Duration::from_secs(3),
      Type::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(2, &mut rx, ReplyCode::CommandOkay, None).await;
    assert_eq!(
      session_properties.read().await.data_type,
      DataType::ASCII {
        sub_type: SubType::TelnetFormatEffectors
      }
    );
  }

  #[tokio::test]
  async fn ascii_cc_test() {
    let ip: SocketAddr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let mut command_processor = CommandProcessor::new(session_properties.clone(), wrapper);

    let command = Command::new(Commands::TYPE, "A C");

    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(_) = timeout(
      Duration::from_secs(3),
      Type::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(2, &mut rx, ReplyCode::CommandOkay, None).await;
    assert_eq!(
      session_properties.read().await.data_type,
      DataType::ASCII {
        sub_type: SubType::CarriageControl
      }
    );
  }

  #[tokio::test]
  async fn empty_test() {
    let ip: SocketAddr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let mut command_processor = CommandProcessor::new(session_properties.clone(), wrapper);

    let command = Command::new(Commands::TYPE, "");

    let original_type = session_properties.read().await.data_type;

    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(_) = timeout(
      Duration::from_secs(3),
      Type::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(
      2,
      &mut rx,
      ReplyCode::SyntaxErrorInParametersOrArguments,
      None,
    )
    .await;
    assert_eq!(session_properties.read().await.data_type, original_type);
  }

  #[tokio::test]
  async fn ebcdic_no_subtype_test() {
    let ip: SocketAddr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let mut command_processor = CommandProcessor::new(session_properties.clone(), wrapper);

    let command = Command::new(Commands::TYPE, "E");

    let original_type = session_properties.read().await.data_type;

    let (tx, mut rx) = mpsc::channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(_) = timeout(
      Duration::from_secs(3),
      Type::execute(&mut command_processor, &command, &mut reply_sender),
    )
      .await
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(
      2,
      &mut rx,
      ReplyCode::CommandNotImplementedForThatParameter,
      None,
    )
      .await;
    assert_eq!(session_properties.read().await.data_type, original_type);
  }
}
