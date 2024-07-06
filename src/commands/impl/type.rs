use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;
use crate::session::data_type::{DataType, SubType};
use std::sync::Arc;

#[tracing::instrument(skip(command_processor, reply_sender))]
pub(crate) async fn r#type(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  debug_assert_eq!(command.command, Commands::Type);

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
        "Mode not specified!",
      ))
      .await;
    return;
  }

  let (new_type, sub_type) = command
    .argument
    .split_once(' ')
    .unwrap_or((&command.argument, ""));

  match (new_type, sub_type) {
    ("A", "N") | ("A", "") => {
      session_properties.data_type = DataType::Ascii {
        sub_type: SubType::NonPrint,
      }
    }
    ("A", "T") => {
      session_properties.data_type = DataType::Ascii {
        sub_type: SubType::TelnetFormatEffectors,
      }
    }
    ("A", "C") => {
      session_properties.data_type = DataType::Ascii {
        sub_type: SubType::CarriageControl,
      }
    }
    ("I", _) => session_properties.data_type = DataType::Binary,
    (_, _) => {
      reply_sender
        .send_control_message(Reply::new(
          ReplyCode::CommandNotImplementedForThatParameter,
          "Invalid or not supported mode!",
        ))
        .await;
      return;
    }
  };

  reply_sender
    .send_control_message(Reply::new(
      ReplyCode::CommandOkay,
      format!("TYPE set to {}.", new_type),
    ))
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
  use crate::session::data_type::{DataType, SubType};
  use crate::utils::test_utils::{
    receive_and_verify_reply, setup_test_command_processor_custom, CommandProcessorSettingsBuilder,
    TestReplySender,
  };

  #[tokio::test]
  async fn ascii_non_print_test() {
    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap())
      .build()
      .expect("Settings should be valid");

    let command_processor = Arc::new(setup_test_command_processor_custom(&settings));

    let command = Command::new(Commands::Type, "A N");

    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    if timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
    )
    .await
    .is_err()
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(2, &mut rx, ReplyCode::CommandOkay, None).await;
    assert_eq!(
      command_processor.session_properties.read().await.data_type,
      DataType::Ascii {
        sub_type: SubType::NonPrint
      }
    );
  }

  #[tokio::test]
  async fn ascii_no_subtype_test() {
    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap())
      .build()
      .expect("Settings should be valid");

    let command_processor = Arc::new(setup_test_command_processor_custom(&settings));

    let command = Command::new(Commands::Type, "A");

    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    if timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
    )
    .await
    .is_err()
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(2, &mut rx, ReplyCode::CommandOkay, None).await;
    assert_eq!(
      command_processor.session_properties.read().await.data_type,
      DataType::Ascii {
        sub_type: SubType::NonPrint
      }
    );
  }

  #[tokio::test]
  async fn binary_test() {
    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap())
      .build()
      .expect("Settings should be valid");

    let command_processor = Arc::new(setup_test_command_processor_custom(&settings));

    let command = Command::new(Commands::Type, "I");

    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::CommandOkay, None).await;
    assert_eq!(
      command_processor.session_properties.read().await.data_type,
      DataType::Binary
    );
  }

  #[tokio::test]
  async fn ascii_tfe_test() {
    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap().join("test_files"))
      .build()
      .expect("Settings should be valid");

    let command_processor = Arc::new(setup_test_command_processor_custom(&settings));

    let command = Command::new(Commands::Type, "A T");

    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::CommandOkay, None).await;
    assert_eq!(
      command_processor.session_properties.read().await.data_type,
      DataType::Ascii {
        sub_type: SubType::TelnetFormatEffectors
      }
    );
  }

  #[tokio::test]
  async fn ascii_cc_test() {
    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap().join("test_files"))
      .build()
      .expect("Settings should be valid");

    let command_processor = Arc::new(setup_test_command_processor_custom(&settings));

    let command = Command::new(Commands::Type, "A C");

    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::CommandOkay, None).await;
    assert_eq!(
      command_processor.session_properties.read().await.data_type,
      DataType::Ascii {
        sub_type: SubType::CarriageControl
      }
    );
  }

  #[tokio::test]
  async fn empty_test() {
    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap().join("test_files"))
      .build()
      .expect("Settings should be valid");

    let command_processor = Arc::new(setup_test_command_processor_custom(&settings));

    let command = Command::new(Commands::Type, "");

    let original_type = command_processor.session_properties.read().await.data_type;

    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
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
    assert_eq!(
      command_processor.session_properties.read().await.data_type,
      original_type
    );
  }

  #[tokio::test]
  async fn ebcdic_no_subtype_test() {
    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap().join("test_files"))
      .build()
      .expect("Settings should be valid");

    let command_processor = Arc::new(setup_test_command_processor_custom(&settings));

    let command = Command::new(Commands::Type, "E");

    let original_type = command_processor.session_properties.read().await.data_type;

    let (tx, mut rx) = mpsc::channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(command_processor.clone(), Arc::new(reply_sender)),
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
    assert_eq!(
      command_processor.session_properties.read().await.data_type,
      original_type
    );
  }
}
