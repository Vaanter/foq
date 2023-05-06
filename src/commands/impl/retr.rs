use async_trait::async_trait;
use tracing::{debug, info, warn};

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::commands::r#impl::shared::{
  get_data_channel_lock, get_open_file_result, get_transfer_reply,
};
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::io::open_options_flags::OpenOptionsWrapperBuilder;
use crate::session::command_processor::CommandProcessor;

pub(crate) struct Retr;

#[async_trait]
impl Executable for Retr {
  #[tracing::instrument(skip(command_processor, reply_sender))]
  async fn execute(
    command_processor: &mut CommandProcessor,
    command: &Command,
    reply_sender: &mut impl ReplySend,
  ) {
    debug_assert_eq!(command.command, Commands::RETR);

    if command.argument.is_empty() {
      Self::reply(
        Reply::new(
          ReplyCode::SyntaxErrorInParametersOrArguments,
          "No file specified!",
        ),
        reply_sender,
      )
      .await;
      return;
    }

    let session_properties = command_processor.session_properties.read().await;

    if !session_properties.is_logged_in() {
      Self::reply(
        Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"),
        reply_sender,
      )
      .await;
      return;
    }

    debug!("Locking data channel!");
    let data_channel_lock = get_data_channel_lock(command_processor.data_wrapper.clone()).await;
    let mut data_channel = match data_channel_lock {
      Ok(dc) => dc,
      Err(e) => {
        Self::reply(e, reply_sender).await;
        return;
      }
    };

    let options = OpenOptionsWrapperBuilder::default()
      .read(true)
      .build()
      .unwrap();
    let file = session_properties
      .file_system_view_root
      .open_file(&command.argument, options)
      .await;
    info!(
      "User '{}' opening file '{}'.",
      session_properties.username.as_ref().unwrap(),
      &command.argument
    );
    let mut file = match get_open_file_result(file) {
      Ok(f) => f,
      Err(reply) => {
        Self::reply(reply, reply_sender).await;
        return;
      }
    };
    debug!("File opened successfully.");

    Self::reply(
      Reply::new(ReplyCode::FileStatusOkay, "Starting file transfer!"),
      reply_sender,
    )
    .await;

    debug!("Sending file data!");
    let success = match tokio::io::copy(&mut file, &mut data_channel.as_mut().unwrap()).await {
      Ok(len) => {
        debug!("Sent {len} bytes.");
        true
      }
      Err(_) => {
        warn!("Error sending file!");
        false
      }
    };

    Self::reply(get_transfer_reply(success), reply_sender).await;

    // Needed to release the lock. Maybe find a better way to do this?
    drop(data_channel);
    command_processor
      .data_wrapper
      .lock()
      .await
      .close_data_stream()
      .await;
  }
}

#[cfg(test)]
mod tests {
  use std::collections::HashSet;
  use std::env::current_dir;
  use std::path::Path;
  use std::sync::Arc;
  use std::time::Duration;

  use blake3::Hasher;
  use s2n_quic::client::Connect;
  use s2n_quic::provider::tls::default::Client as TlsClient;
  use s2n_quic::Client;
  use tokio::fs::OpenOptions;
  use tokio::io::{AsyncRead, AsyncReadExt};
  use tokio::sync::mpsc::channel;
  use tokio::sync::{Mutex, RwLock};
  use tokio::time::timeout;
  use tokio_util::sync::CancellationToken;

  use crate::auth::user_permission::UserPermission;
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::retr::Retr;
  use crate::commands::reply_code::ReplyCode;
  use crate::data_channels::data_channel_wrapper::DataChannelWrapper;
  use crate::data_channels::quic_only_data_channel_wrapper::QuicOnlyDataChannelWrapper;
  use crate::data_channels::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::file_system_view::FileSystemView;
  use crate::listeners::quic_only_listener::QuicOnlyListener;
  use crate::session::command_processor::CommandProcessor;
  use crate::session::session_properties::SessionProperties;
  use crate::utils::test_utils::{
    create_tls_client_config, open_tcp_data_channel, receive_and_verify_reply,
    setup_test_command_processor, TestReplySender, LOCALHOST,
  };

  async fn common_tcp(file_name: &'static str) {
    let wrapper = StandardDataChannelWrapper::new(LOCALHOST);
    let (command, mut command_processor) = setup(&file_name, wrapper);
    let client_dc = open_tcp_data_channel(&mut command_processor).await;
    transfer(&file_name, command, command_processor, client_dc).await;
  }

  async fn common_quic(file_name: &'static str) {
    let mut listener = QuicOnlyListener::new(LOCALHOST).unwrap();
    let addr = listener.server.local_addr().unwrap();
    let token = CancellationToken::new();
    let test_handle = tokio::spawn(async move {
      let connection = listener.accept(token.clone()).await.unwrap();
      let wrapper = QuicOnlyDataChannelWrapper::new(LOCALHOST, Arc::new(Mutex::new(connection)));
      let (command, command_processor) = setup(&file_name, wrapper);
      (command, command_processor)
    });

    let tls_client = TlsClient::new(create_tls_client_config());

    let client = Client::builder()
      .with_tls(tls_client)
      .expect("Client requires valid TLS settings!")
      .with_io(LOCALHOST)
      .expect("Client requires valid I/O settings!")
      .start()
      .expect("Client must be able to start");

    let connect = Connect::new(addr).with_server_name("localhost");
    let mut client_connection = match timeout(Duration::from_secs(2), client.connect(connect)).await
    {
      Ok(Ok(conn)) => conn,
      Ok(Err(e)) => panic!("Client failed to connect to the server! {}", e),
      Err(_) => panic!("Client failed to connect to the server in time!"),
    };

    let (command, command_processor) = match timeout(Duration::from_secs(1), test_handle).await {
      Ok(Ok(c)) => c,
      Ok(Err(_)) => panic!("Future error!"),
      Err(_) => panic!("Connection setup failed!"),
    };

    let _ = command_processor
      .data_wrapper
      .lock()
      .await
      .open_data_stream()
      .await
      .unwrap();

    let client_dc = match client_connection.open_bidirectional_stream().await {
      Ok(client_dc) => client_dc,
      Err(e) => panic!("Failed to open data channel! Error: {}", e),
    };

    transfer(&file_name, command, command_processor, client_dc).await;
  }

  fn setup<T: DataChannelWrapper + 'static>(
    file_name: &str,
    data_channel_wrapper: T,
  ) -> (Command, CommandProcessor) {
    eprintln!("Running setup.");
    if !Path::new(&file_name).exists() {
      panic!("Test file does not exist! Cannot proceed!");
    }

    let command = Command::new(Commands::RETR, file_name);

    let label = "test";
    let view = FileSystemView::new(
      current_dir().unwrap(),
      label.clone(),
      HashSet::from([UserPermission::READ]),
    );

    let mut session_properties = SessionProperties::new();
    session_properties
      .file_system_view_root
      .set_views(vec![view]);
    session_properties
      .file_system_view_root
      .change_working_directory(label);
    let _ = session_properties.username.insert("test".to_string());

    let session_properties = Arc::new(RwLock::new(session_properties));
    let wrapper = Arc::new(Mutex::new(data_channel_wrapper));
    let command_processor = CommandProcessor::new(session_properties.clone(), wrapper);
    println!("Setup completed.");
    (command, command_processor)
  }

  async fn transfer<T: AsyncRead + Unpin>(
    file_name: &str,
    command: Command,
    mut command_processor: CommandProcessor,
    mut client_dc: T,
  ) {
    println!("Running transfer.");
    // TODO adjust timeout better maybe?
    let timeout_secs = 1
      + Path::new(&file_name)
        .metadata()
        .expect("Metadata should be accessible!")
        .len()
        .ilog10();

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);

    let command_fut = tokio::spawn(async move {
      if let Err(_) = timeout(
        Duration::from_secs(timeout_secs as u64),
        Retr::execute(&mut command_processor, &command, &mut reply_sender),
      )
      .await
      {
        panic!("Command timeout!");
      };
    });

    receive_and_verify_reply(2, &mut rx, ReplyCode::FileStatusOkay, None).await;

    let transfer = verify_transfer(&file_name, &mut client_dc);

    match timeout(Duration::from_secs(5), transfer).await {
      Ok(()) => println!("Transfer complete!"),
      Err(_) => panic!("Transfer timed out!"),
    }

    receive_and_verify_reply(2, &mut rx, ReplyCode::ClosingDataConnection, None).await;

    command_fut.await.expect("Command should complete!");
  }

  async fn verify_transfer<T: AsyncRead + Unpin>(file_name: &str, client_dc: &mut T) {
    let mut local_file_hasher = Hasher::new();
    let mut sent_file_hasher = Hasher::new();

    let mut local_file = OpenOptions::new()
      .read(true)
      .open(&file_name)
      .await
      .expect("Test file must exist!");

    let mut transfer_buffer = [0; 1024];
    let mut local_buffer = [0; 1024];
    loop {
      let transfer_len = match client_dc.read(&mut transfer_buffer).await {
        Ok(len) => len,
        Err(e) => panic!("File transfer failed! {e}"),
      };
      let local_len = match local_file.read(&mut local_buffer).await {
        Ok(len) => len,
        Err(e) => panic!("Failed to read local file! {e}"),
      };

      assert_eq!(transfer_len, local_len);
      if local_len == 0 || transfer_len == 0 {
        break;
      }

      sent_file_hasher.update(&transfer_buffer[..transfer_len]);
      local_file_hasher.update(&local_buffer[..local_len]);
    }
    assert_eq!(
      local_file_hasher.finalize(),
      sent_file_hasher.finalize(),
      "File hashes do not match!"
    );
    println!("File hashes match!");
  }

  #[tokio::test]
  async fn test_two_kib() {
    const FILE_NAME: &'static str = "test_files/2KiB.txt";
    common_tcp(FILE_NAME).await;
  }

  #[tokio::test]
  async fn test_one_mib() {
    const FILE_NAME: &'static str = "test_files/1MiB.txt";
    common_tcp(FILE_NAME).await;
  }

  #[tokio::test]
  async fn test_ten_paragraphs() {
    const FILE_NAME: &'static str = "test_files/lorem_10_paragraphs.txt";
    common_tcp(FILE_NAME).await;
  }

  #[tokio::test]
  async fn two_kib_quic_test() {
    const FILE_NAME: &'static str = "test_files/2KiB.txt";
    timeout(Duration::from_secs(2), common_quic(FILE_NAME))
      .await
      .unwrap();
  }

  #[tokio::test]
  async fn one_mib_quic_test() {
    const FILE_NAME: &'static str = "test_files/1MiB.txt";
    common_quic(FILE_NAME).await;
  }

  #[tokio::test]
  async fn ten_paragraphs_quic_test() {
    const FILE_NAME: &'static str = "test_files/lorem_10_paragraphs.txt";
    common_quic(FILE_NAME).await;
  }

  #[tokio::test]
  async fn not_logged_in_test() {
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let command = Command::new(Commands::RETR, "NONEXISTENT");
    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      Retr::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::NotLoggedIn, None).await;
  }

  #[tokio::test]
  async fn data_channel_not_open_test() {
    let (label, mut command_processor) = setup_test_command_processor();

    let command = Command::new(
      Commands::RETR,
      format!("{}/test_files/1MiB.txt", label.clone()),
    );
    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      Retr::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::BadSequenceOfCommands, None).await;
  }

  #[tokio::test]
  async fn no_file_specified_test() {
    let (_, mut command_processor) = setup_test_command_processor();

    let _ = open_tcp_data_channel(&mut command_processor).await;
    let command = Command::new(Commands::RETR, "");
    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      Retr::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(
      2,
      &mut rx,
      ReplyCode::SyntaxErrorInParametersOrArguments,
      None,
    )
    .await;
  }

  #[tokio::test]
  async fn invalid_file_test() {
    let (label, mut command_processor) = setup_test_command_processor();

    let _ = open_tcp_data_channel(&mut command_processor).await;
    let command = Command::new(
      Commands::RETR,
      format!("{}/test_files/NONEXISTENT", label.clone()),
    );
    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      Retr::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::FileUnavailable, None).await;
  }

  #[tokio::test]
  async fn no_read_permission_test() {
    let (label, mut command_processor) = setup_test_command_processor();
    command_processor
      .session_properties
      .write()
      .await
      .file_system_view_root
      .file_system_views
      .as_mut()
      .unwrap()
      .iter_mut()
      .for_each(|v| v.1.permissions.clear());

    let _ = open_tcp_data_channel(&mut command_processor).await;
    let command = Command::new(
      Commands::RETR,
      format!("{}/test_files/2KiB.txt", label.clone()),
    );
    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      Retr::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::FileUnavailable, None).await;
  }

  #[tokio::test]
  async fn folder_specified_test() {
    let (label, mut command_processor) = setup_test_command_processor();

    let _ = open_tcp_data_channel(&mut command_processor).await;
    let command = Command::new(Commands::RETR, format!("{}/test_files", label.clone()));
    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      Retr::execute(&mut command_processor, &command, &mut reply_sender),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(
      2,
      &mut rx,
      ReplyCode::SyntaxErrorInParametersOrArguments,
      None,
    )
    .await;
  }
}
