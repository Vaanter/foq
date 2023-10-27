use tokio::io::AsyncWriteExt;
use tracing::{debug, info, trace};

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::r#impl::shared::get_listing_or_error_reply;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::io::entry_data::EntryType;
use crate::session::command_processor::CommandProcessor;

pub(crate) async fn nlst(
  command: &Command,
  command_processor: &mut CommandProcessor,
  reply_sender: &mut impl ReplySend,
) {
  debug_assert_eq!(Commands::Nlst, command.command);

  let session_properties = command_processor.session_properties.read().await;

  if !session_properties.is_logged_in() {
    reply_sender
      .send_control_message(Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"))
      .await;
    return;
  }

  let listing = session_properties
    .file_system_view_root
    .list_dir(&command.argument);

  let listing = match get_listing_or_error_reply(listing) {
    Ok(l) => l,
    Err(r) => return reply_sender.send_control_message(r).await,
  };

  debug!("Locking data stream!");
  let stream = command_processor
    .data_wrapper
    .lock()
    .await
    .get_data_stream()
    .await;

  match stream.lock().await.as_mut() {
    Some(s) => {
      reply_sender
        .send_control_message(Reply::new(
          ReplyCode::FileStatusOkay,
          "Transferring directory information!",
        ))
        .await;
      let mem = listing
        .iter()
        .filter(|l| l.entry_type() != EntryType::Cdir)
        .fold(String::with_capacity(listing.len() * 32), |mut acc, e| {
          acc.push_str(&format!(" {}\r\n", e.name()));
          acc
        });
      trace!("Sending listing to client:\n{}", mem);
      let len = s.write_all(mem.as_ref()).await;
      debug!("Sending listing result: {:?}", len);
    }
    None => {
      info!("Data stream is not open!");
      reply_sender
        .send_control_message(Reply::new(
          ReplyCode::BadSequenceOfCommands,
          "Data connection is not open!",
        ))
        .await;
      return;
    }
  }

  debug!("Listing sent to client!");
  reply_sender
    .send_control_message(Reply::new(
      ReplyCode::ClosingDataConnection,
      "Directory information sent!",
    ))
    .await;
  command_processor
    .data_wrapper
    .lock()
    .await
    .close_data_stream()
    .await;
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use tokio::io::AsyncReadExt;
  use tokio::sync::mpsc::channel;
  use tokio::time::timeout;

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::reply_code::ReplyCode;
  use crate::utils::test_utils::{
    open_tcp_data_channel, receive_and_verify_reply, setup_test_command_processor,
    setup_test_command_processor_custom, CommandProcessorSettingsBuilder, TestReplySender,
  };

  #[tokio::test]
  async fn list_directory_test() {
    let command = Command::new(Commands::Nlst, String::new());
    let (_, mut command_processor) = setup_test_command_processor();

    let mut client_dc = open_tcp_data_channel(&mut command_processor).await;

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(&mut command_processor, &mut reply_sender),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::FileStatusOkay, None).await;

    let mut buffer = [0; 1024];
    match timeout(Duration::from_secs(5), client_dc.read(&mut buffer)).await {
      Ok(Ok(len)) => {
        let msg = String::from_utf8_lossy(&buffer[..len]);
        assert!(!msg.is_empty());

        let file_count = 1;

        println!("Data:\n{}", msg);
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
  async fn not_logged_in_test() {
    let command = Command::new(Commands::Nlst, String::new());
    let settings = CommandProcessorSettingsBuilder::default()
      .build()
      .expect("Settings should be valid");
    let mut command_processor = setup_test_command_processor_custom(&settings);

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(&mut command_processor, &mut reply_sender),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::NotLoggedIn, None).await;
  }

  #[tokio::test]
  async fn data_connection_not_open_test() {
    let command = Command::new(Commands::Nlst, String::new());
    let (_, mut command_processor) = setup_test_command_processor();

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(3),
      command.execute(&mut command_processor, &mut reply_sender),
    )
    .await
    .expect("Command timeout!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::BadSequenceOfCommands, None).await;
  }
}
