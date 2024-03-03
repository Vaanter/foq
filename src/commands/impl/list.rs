use regex::Regex;
use std::io::ErrorKind;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::select;
use tracing::{debug, trace, warn};

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::r#impl::shared::{acquire_data_channel, get_listing_or_error_reply};
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::io::entry_data::EntryType;
use crate::session::command_processor::CommandProcessor;

#[tracing::instrument(skip(command_processor, reply_sender))]
pub(crate) async fn list(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  debug_assert_eq!(command.command, Commands::List);

  let session_properties = command_processor.session_properties.read().await;

  if !session_properties.is_logged_in() {
    reply_sender
      .send_control_message(Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"))
      .await;
    return;
  }

  let arguments_re = Regex::new("^(-[al])? ?(.*)?$").expect("Regex should be valid!");

  let path = match arguments_re.captures(&command.argument) {
    Some(caps) => match caps.get(2) {
      Some(p) => p.as_str(),
      None => ".",
    },
    None => ".",
  };

  let listing = session_properties.file_system_view_root.list_dir(path);

  let listing = match get_listing_or_error_reply(listing) {
    Ok(l) => l,
    Err(r) => return reply_sender.send_control_message(r).await,
  };

  let data_channel_pair = acquire_data_channel(command_processor.data_wrapper.clone()).await;
  let (mut data_channel, token) = match data_channel_pair {
    Ok((dc, token)) => (dc, token),
    Err(e) => {
      return reply_sender.send_control_message(e).await;
    }
  };

  reply_sender
    .send_control_message(Reply::new(
      ReplyCode::FileStatusOkay,
      "Transferring directory information!",
    ))
    .await;

  let mem = listing
    .iter()
    .filter(|l| l.entry_type() != EntryType::Cdir)
    .map(|l| l.to_list_string())
    .collect::<String>();
  trace!(
    "Sending listing to client:\n{}",
    mem.replace("\r\n", "\\r\\n")
  );
  let transfer = data_channel.write_all(mem.as_ref());
  let result = select! {
    result = transfer => result,
    _ = token.cancelled() => Err(std::io::Error::new(ErrorKind::ConnectionAborted, "Connection aborted!"))
  };
  debug!("Sending listing result: {:?}", result);
  debug!("Flushing data");
  if let Err(e) = data_channel.flush().await {
    warn!("Failed to flush data after writing! {e}");
  }
  if let Err(e) = data_channel.shutdown().await {
    warn!("Failed to shutdown data channel after writing! {e}");
  }

  debug!("Listing sent to client!");
  reply_sender
    .send_control_message(Reply::new(
      ReplyCode::ClosingDataConnection,
      "Directory information sent!",
    ))
    .await;
}

#[cfg(test)]
mod tests {
  use std::collections::HashSet;
  use std::env::current_dir;
  use std::sync::Arc;
  use std::time::Duration;

  use regex::Regex;
  use tokio::io::AsyncReadExt;
  use tokio::sync::mpsc::channel;
  use tokio::time::timeout;

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::r#impl::shared::ACQUIRE_TIMEOUT;
  use crate::commands::reply_code::ReplyCode;
  use crate::utils::test_utils::*;

  async fn listing_common(command: Command, settings: &CommandProcessorSettings) {
    let mut command_processor = setup_test_command_processor_custom(settings);

    let mut client_dc = open_tcp_data_channel(&mut command_processor).await;

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::FileStatusOkay, None).await;

    let mut buffer = [0; 2048];
    match timeout(Duration::from_secs(5), client_dc.read(&mut buffer)).await {
      Ok(Ok(len)) => {
        let msg = String::from_utf8_lossy(&buffer[..len]);
        assert!(!msg.is_empty());

        let file_count = settings
          .view_root
          .read_dir()
          .expect("Failed to read current path!")
          .count();

        println!("Message:\n{}", msg);

        let re = Regex::new(r"^[dl-](?:[r-][w-][x-]){3} 1 user group  {0,12}\d{1,20} [A-Za-z]{3} [0-3][0-9] (?:(?:[01][0-9]|2[0-4]):[0-5][0-9]|[12][0-9]{3}) .*$").unwrap();

        for line in msg.lines() {
          assert!(re.is_match(line), "Invalid line: '{line}'");
        }
        assert_eq!(file_count, msg.lines().count());
      }
      Ok(Err(e)) => {
        panic!("Transfer error: {}", e);
      }
      Err(_) => {
        panic!("Transfer timed out.");
      }
    };

    receive_and_verify_reply(2, &mut rx, ReplyCode::ClosingDataConnection, None).await;
  }

  #[tokio::test]
  async fn simple_listing_tcp() {
    let command = Command::new(Commands::List, String::new());

    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap().join("test_files"))
      .build()
      .expect("Settings should be valid");

    listing_common(command, &settings).await;
  }

  #[tokio::test]
  async fn listing_with_argument_tcp() {
    let command = Command::new(Commands::List, "-a".to_string());

    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap().join("test_files"))
      .build()
      .expect("Settings should be valid");

    listing_common(command, &settings).await;
  }

  #[tokio::test]
  async fn listing_with_path_parameter_tcp() {
    let command = Command::new(Commands::List, ".".to_string());

    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap().join("test_files"))
      .build()
      .expect("Settings should be valid");

    listing_common(command, &settings).await;
  }

  #[tokio::test]
  async fn listing_with_argument_with_path_parameter_tcp() {
    let command = Command::new(Commands::List, "-l .".to_string());

    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap().join("test_files"))
      .build()
      .expect("Settings should be valid");

    listing_common(command, &settings).await;
  }

  #[tokio::test]
  async fn not_logged_in_test() {
    let command = Command::new(Commands::List, String::new());

    let settings = CommandProcessorSettingsBuilder::default()
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
    .expect("Command should finish before timeout");
    receive_and_verify_reply(2, &mut rx, ReplyCode::NotLoggedIn, None).await;
  }

  #[tokio::test]
  async fn not_directory_test() {
    let command = Command::new(Commands::List, String::from("1MiB.txt"));

    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap().join("test_files"))
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
    .expect("Command should finish before timeout");

    receive_and_verify_reply(
      2,
      &mut rx,
      ReplyCode::SyntaxErrorInParametersOrArguments,
      None,
    )
    .await;
  }

  #[tokio::test]
  async fn nonexistent_test() {
    let command = Command::new(Commands::List, String::from("NONEXISTENT"));

    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap().join("test_files"))
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

    receive_and_verify_reply(2, &mut rx, ReplyCode::FileUnavailable, None).await;
  }

  #[tokio::test]
  async fn insufficient_permissions_test() {
    let command = Command::new(Commands::List, String::new());

    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap().join("test_files"))
      .permissions(HashSet::new())
      .build()
      .expect("Settings should be valid");

    let command_processor = setup_test_command_processor_custom(&settings);

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    if (timeout(
      Duration::from_secs(3),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await)
      .is_err()
    {
      panic!("Command timeout!");
    };

    receive_and_verify_reply(
      2,
      &mut rx,
      ReplyCode::FileUnavailable,
      Some("Insufficient permissions!"),
    )
    .await;
  }

  #[tokio::test]
  async fn data_channel_not_open_tcp() {
    let command = Command::new(Commands::List, String::new());

    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap().join("test_files"))
      .build()
      .expect("Settings should be valid");

    let command_processor = setup_test_command_processor_custom(&settings);

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(ACQUIRE_TIMEOUT + 3),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::BadSequenceOfCommands, None).await;
  }
}
