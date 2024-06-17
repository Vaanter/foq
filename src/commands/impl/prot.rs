use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::global_context::TLS_CONFIG;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;
use crate::session::protection_mode::ProtMode;
use std::str::FromStr;
use std::sync::Arc;

#[tracing::instrument(skip(command_processor, reply_sender))]
pub(crate) async fn prot(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  debug_assert_eq!(command.command, Commands::Prot);

  let mut properties = command_processor.session_properties.write().await;

  let reply = match ProtMode::from_str(&command.argument) {
    Ok(ProtMode::Clear) => {
      properties.prot_mode = ProtMode::Clear;
      Reply::new(ReplyCode::CommandOkay, "Protection level set")
    }
    Ok(ProtMode::Safe) | Ok(ProtMode::Confidential) => Reply::new(
      ReplyCode::ProtectionLevelNotSupported,
      "Protection mode not available",
    ),
    Ok(ProtMode::Private) => {
      if TLS_CONFIG.clone().is_some() {
        properties.prot_mode = ProtMode::Private;
        Reply::new(ReplyCode::CommandOkay, "Protection level set")
      } else {
        Reply::new(ReplyCode::AuthNotAvailable, "Protection not available")
      }
    }
    Err(_) => Reply::new(
      ReplyCode::CommandNotImplementedForThatParameter,
      "Unknown protection mode",
    ),
  };

  reply_sender.send_control_message(reply).await;
}

#[cfg(test)]
mod tests {
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::reply_code::ReplyCode;
  use crate::session::protection_mode::ProtMode;
  use crate::utils::test_utils::{
    receive_and_verify_reply, setup_test_command_processor_custom, CommandProcessorSettingsBuilder,
    TestReplySender,
  };
  use std::sync::Arc;
  use std::time::Duration;
  use tokio::sync::mpsc::channel;
  use tokio::time::timeout;

  #[tokio::test]
  async fn empty_argument_test() {
    let command = Command::new(Commands::Prot, "");

    let settings = CommandProcessorSettingsBuilder::default()
      .username(Some("testuser".to_string()))
      .build()
      .expect("Settings should be valid");

    let command_processor = setup_test_command_processor_custom(&settings);

    let (tx, mut rx) = channel(1024);
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
      ReplyCode::CommandNotImplementedForThatParameter,
      None,
    )
    .await;
  }

  #[tokio::test]
  async fn set_private_test() {
    let command = Command::new(Commands::Prot, "P");

    let settings = CommandProcessorSettingsBuilder::default()
      .username(Some("testuser".to_string()))
      .build()
      .expect("Settings should be valid");

    let command_processor = Arc::new(setup_test_command_processor_custom(&settings));

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::CommandOkay, None).await;
    assert_eq!(
      ProtMode::Private,
      command_processor.session_properties.read().await.prot_mode
    );
  }

  #[tokio::test]
  async fn set_clear_from_private_test() {
    let command = Command::new(Commands::Prot, "C");

    let settings = CommandProcessorSettingsBuilder::default()
      .username(Some("testuser".to_string()))
      .build()
      .expect("Settings should be valid");

    let command_processor = Arc::new(setup_test_command_processor_custom(&settings));
    command_processor.session_properties.write().await.prot_mode = ProtMode::Private;

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::CommandOkay, None).await;
    assert_eq!(
      ProtMode::Clear,
      command_processor.session_properties.read().await.prot_mode
    );
  }

  #[tokio::test]
  async fn set_safe_test() {
    let command = Command::new(Commands::Prot, "S");

    let settings = CommandProcessorSettingsBuilder::default()
      .username(Some("testuser".to_string()))
      .build()
      .expect("Settings should be valid");

    let command_processor = Arc::new(setup_test_command_processor_custom(&settings));

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::ProtectionLevelNotSupported, None).await;
    assert_eq!(
      ProtMode::Clear,
      command_processor.session_properties.read().await.prot_mode
    );
  }
}
