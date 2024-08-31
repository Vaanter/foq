use std::io::{ErrorKind, SeekFrom};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::io::{AsyncSeekExt, AsyncWriteExt, BufReader};
use tokio::select;
use tracing::{debug, info, warn};

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::r#impl::shared::{
  acquire_data_channel, copy_data, get_open_file_result, get_transfer_reply, TRANSFER_BUFFER_SIZE,
};
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::io::open_options_flags::OpenOptionsWrapperBuilder;
use crate::session::command_processor::CommandProcessor;

#[tracing::instrument(skip(command_processor, reply_sender))]
pub(crate) async fn retr(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  debug_assert_eq!(command.command, Commands::Retr);

  let session_properties = command_processor.session_properties.read().await;

  if command.argument.is_empty() {
    return reply_sender
      .send_control_message(Reply::new(
        ReplyCode::SyntaxErrorInParametersOrArguments,
        "No file specified!",
      ))
      .await;
  }

  if !session_properties.is_logged_in() {
    return reply_sender
      .send_control_message(Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"))
      .await;
  }

  let data_channel_pair = acquire_data_channel(command_processor.data_wrapper.clone()).await;
  let (mut data_channel, token) = match data_channel_pair {
    Ok((dc, token)) => (dc, token),
    Err(e) => {
      return reply_sender.send_control_message(e).await;
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
      reply_sender.send_control_message(reply).await;
      return;
    }
  };
  debug!("File opened successfully.");

  reply_sender
    .send_control_message(Reply::new(
      ReplyCode::FileStatusOkay,
      "Starting file transfer!",
    ))
    .await;

  let offset = session_properties.offset.swap(0, Ordering::SeqCst);
  if offset > 0 {
    debug!("Setting cursor to offset: {}", offset);
    if let Err(e) = file.seek(SeekFrom::Start(offset)).await {
      warn!(
        "Failed to seek file {} to offset {}. Error: {}",
        &command.argument, offset, e
      );
    };
  }

  debug!("Sending file data, offset: {}!", offset);

  let mut buf = BufReader::with_capacity(TRANSFER_BUFFER_SIZE, &mut file);
  let transfer = copy_data(&mut buf, &mut data_channel);

  let success = select! {
    result = transfer => result,
    _ = token.cancelled() => {
      debug!("Received transfer abort");
      Err(std::io::Error::new(ErrorKind::ConnectionAborted, "Connection aborted!"))
    }
  };

  reply_sender
    .send_control_message(get_transfer_reply(&success))
    .await;

  if success.is_ok() {
    if let Err(e) = data_channel.shutdown().await {
      warn!("Failed to shutdown data channel after writing! {e}");
    }
  }
}

#[cfg(test)]
mod tests {
  use std::collections::HashSet;
  use std::env::{current_dir, temp_dir};
  use std::path::PathBuf;
  use std::sync::Arc;
  use std::time::Duration;

  use blake3::Hasher;
  use quinn::VarInt;
  use s2n_quic::client::Connect;
  use tokio::fs::OpenOptions;
  use tokio::io::{AsyncRead, AsyncReadExt};
  use tokio::sync::mpsc::channel;
  use tokio::sync::Mutex;
  use tokio::time::timeout;
  use tokio_util::sync::CancellationToken;
  use uuid::Uuid;

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::r#impl::shared::ACQUIRE_TIMEOUT;
  use crate::commands::reply_code::ReplyCode;
  use crate::data_channels::quic_only_data_channel_wrapper::QuicOnlyDataChannelWrapper;
  use crate::data_channels::quic_quinn_data_channel_wrapper::QuicQuinnDataChannelWrapper;
  use crate::data_channels::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::listeners::quic_only_listener::QuicOnlyListener;
  use crate::listeners::quinn_listener::QuinnListener;
  use crate::session::command_processor::CommandProcessor;
  use crate::session::protection_mode::ProtMode;
  use crate::utils::test_utils::*;

  async fn common_tcp(root: PathBuf, argument: &str) {
    let file_path = root.join(argument);
    assert!(
      file_path.exists(),
      "Test file does not exist! Cannot proceed!"
    );

    let command = Command::new(Commands::Retr, argument);

    let wrapper = StandardDataChannelWrapper::new(LOCALHOST);
    let mut command_processor = setup_transfer_command_processor(wrapper, root);
    let client_dc = open_tcp_data_channel(&mut command_processor).await;
    transfer(file_path, command, command_processor, client_dc).await;
  }

  async fn common_quic(root: PathBuf, argument: &str) {
    let file_path = root.join(argument);
    assert!(
      file_path.exists(),
      "Test file does not exist! Cannot proceed!"
    );

    let mut listener = QuicOnlyListener::new(LOCALHOST).unwrap();
    let addr = listener.server.local_addr().unwrap();
    let token = CancellationToken::new();
    let command = Command::new(Commands::Retr, argument);
    let test_handle = tokio::spawn(async move {
      let connection = listener.accept(token.clone()).await.unwrap();
      let wrapper = QuicOnlyDataChannelWrapper::new(LOCALHOST, Arc::new(Mutex::new(connection)));
      setup_transfer_command_processor(wrapper, root)
    });

    let client = setup_s2n_client();

    let connect = Connect::new(addr).with_server_name("localhost");
    let mut client_connection = match timeout(Duration::from_secs(2), client.connect(connect)).await
    {
      Ok(Ok(conn)) => conn,
      Ok(Err(e)) => panic!("Client failed to connect to the server! {}", e),
      Err(_) => panic!("Client failed to connect to the server in time!"),
    };

    client_connection.keep_alive(true).unwrap();

    let command_processor = match timeout(Duration::from_secs(1), test_handle).await {
      Ok(Ok(c)) => c,
      Ok(Err(_)) => panic!("Future error!"),
      Err(_) => panic!("Connection setup failed!"),
    };

    let _ = command_processor
      .data_wrapper
      .open_data_stream(ProtMode::Clear)
      .await
      .unwrap();

    let client_dc = match client_connection.open_bidirectional_stream().await {
      Ok(client_dc) => client_dc,
      Err(e) => panic!("Failed to open data channel! Error: {}", e),
    };

    transfer(file_path, command, command_processor, client_dc).await;
  }

  async fn common_quinn_quinn(root: PathBuf, argument: &str) {
    let file_path = root.join(argument);
    assert!(
      file_path.exists(),
      "Test file does not exist! Cannot proceed!"
    );

    let listener = QuinnListener::new(LOCALHOST).unwrap();
    let addr = listener.listener.local_addr().unwrap();
    let command = Command::new(Commands::Retr, argument);
    let token = CancellationToken::new();
    let test_handle = tokio::spawn(async move {
      let connection = Arc::new(Mutex::new(
        listener.accept(token.clone()).await.unwrap().await.unwrap(),
      ));
      let wrapper = QuicQuinnDataChannelWrapper::new(LOCALHOST, connection.clone());
      (setup_transfer_command_processor(wrapper, root), connection)
    });

    let tls_config = create_tls_client_config("ftpoq-1");
    let quinn_client = setup_quinn_client(tls_config);

    let connection = match quinn_client.connect(addr, "localhost").unwrap().await {
      Ok(conn) => conn,
      Err(e) => {
        panic!("Client failed to connect to the server! {}", e);
      }
    };

    let (command_processor, server_connection) =
      match timeout(Duration::from_secs(1), test_handle).await {
        Ok(Ok(c)) => c,
        Ok(Err(_)) => panic!("Future error!"),
        Err(_) => panic!("Connection setup failed!"),
      };

    let _ = command_processor
      .data_wrapper
      .open_data_stream(ProtMode::Clear)
      .await
      .unwrap();

    let (mut send_stream, cliend_dc_recv) = match connection.open_bi().await {
      Ok(client_dc) => client_dc,
      Err(e) => panic!("Failed to open data channel! Error: {}", e),
    };
    // Required to actually open the stream
    send_stream.write("".as_bytes()).await.unwrap();

    transfer(file_path, command, command_processor, cliend_dc_recv).await;
    server_connection
      .lock()
      .await
      .close(VarInt::from_u32(0), "Test end".as_bytes());
  }

  async fn common_quic_quinn(root: PathBuf, argument: &str) {
    let file_path = root.join(argument);
    assert!(
      file_path.exists(),
      "Test file does not exist! Cannot proceed!"
    );

    let mut listener = QuicOnlyListener::new(LOCALHOST).unwrap();
    let addr = listener.server.local_addr().unwrap();
    let command = Command::new(Commands::Retr, argument);
    let token = CancellationToken::new();
    let test_handle = tokio::spawn(async move {
      let connection = Arc::new(Mutex::new(listener.accept(token.clone()).await.unwrap()));
      let wrapper = QuicOnlyDataChannelWrapper::new(LOCALHOST, connection.clone());
      (setup_transfer_command_processor(wrapper, root), connection)
    });

    let tls_config = create_tls_client_config("ftpoq-1");
    let quinn_client = setup_quinn_client(tls_config);

    let connection = match quinn_client.connect(addr, "localhost").unwrap().await {
      Ok(conn) => conn,
      Err(e) => {
        panic!("Client failed to connect to the server! {}", e);
      }
    };

    let (command_processor, server_connection) =
      match timeout(Duration::from_secs(1), test_handle).await {
        Ok(Ok(c)) => c,
        Ok(Err(_)) => panic!("Future error!"),
        Err(_) => panic!("Connection setup failed!"),
      };

    let _ = command_processor
      .data_wrapper
      .open_data_stream(ProtMode::Clear)
      .await
      .unwrap();

    let (_, cliend_dc_recv) = match connection.open_bi().await {
      Ok(client_dc) => client_dc,
      Err(e) => panic!("Failed to open data channel! Error: {}", e),
    };

    transfer(file_path, command, command_processor, cliend_dc_recv).await;
    server_connection.lock().await.close(0u32.into());
    //println!("stats: {:#?}", connection);
  }

  async fn transfer<T: AsyncRead + Unpin>(
    file_path: PathBuf,
    command: Command,
    command_processor: CommandProcessor,
    mut client_dc: T,
  ) {
    println!("Running transfer.");
    const TIMEOUT_SECS: u64 = 600;

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);

    let command_fut = tokio::spawn(async move {
      timeout(
        Duration::from_secs(TIMEOUT_SECS),
        command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
      )
      .await
      .expect("Command timeout!");
    });

    receive_and_verify_reply(2, &mut rx, ReplyCode::FileStatusOkay, None).await;

    let transfer = verify_transfer(file_path, &mut client_dc);

    match timeout(Duration::from_secs(TIMEOUT_SECS), transfer).await {
      Ok(()) => println!("Transfer complete!"),
      Err(_) => panic!("Transfer timed out!"),
    }

    receive_and_verify_reply(2, &mut rx, ReplyCode::ClosingDataConnection, None).await;

    command_fut.await.expect("Command should complete!");
  }

  async fn verify_transfer<T: AsyncRead + Unpin>(file_path: PathBuf, client_dc: &mut T) {
    let mut local_file_hasher = Hasher::new();
    let mut sent_file_hasher = Hasher::new();

    let mut local_file = OpenOptions::new()
      .read(true)
      .open(file_path)
      .await
      .expect("Test file must exist!");

    const BUFFER_MAX: usize = 8096;
    let mut transfer_buffer = [0; BUFFER_MAX];
    loop {
      let transfer_len = match client_dc.read(&mut transfer_buffer).await {
        Ok(len) => len,
        Err(e) => panic!("File transfer failed! {e:#?}"),
      };
      let mut local_buffer = vec![0u8; transfer_len];
      let local_len = match local_file.read_exact(&mut local_buffer).await {
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
    println!("File hashes match! Hash: {}", sent_file_hasher.finalize());
  }

  #[tokio::test]
  async fn two_kib_test() {
    const FILE_NAME: &str = "test_files/2KiB.txt";
    common_tcp(current_dir().unwrap(), FILE_NAME).await;
  }

  #[tokio::test]
  async fn one_mib_test() {
    const FILE_NAME: &str = "test_files/1MiB.txt";
    common_tcp(current_dir().unwrap(), FILE_NAME).await;
  }

  #[tokio::test]
  async fn ten_paragraphs_test() {
    const FILE_NAME: &str = "test_files/lorem_10_paragraphs.txt";
    common_tcp(current_dir().unwrap(), FILE_NAME).await;
  }

  #[tokio::test]
  #[ignore]
  async fn hundred_mib_test() {
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = temp_dir().join(&file_name);
    let _cleanup = FileCleanup::new(&file_path);
    generate_test_file((100 * 2u64.pow(20)) as usize, &file_path).await;
    common_tcp(temp_dir(), &file_name).await;
  }

  #[tokio::test]
  #[ignore]
  async fn one_gib_test() {
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = temp_dir().join(&file_name);
    let _cleanup = FileCleanup::new(&file_path);
    generate_test_file(2u64.pow(30) as usize, &file_path).await;
    common_tcp(temp_dir(), &file_name).await;
  }

  #[tokio::test]
  async fn two_kib_quic_test() {
    const FILE_NAME: &str = "test_files/2KiB.txt";
    common_quic(current_dir().unwrap(), FILE_NAME).await;
  }

  #[tokio::test]
  async fn one_mib_quic_test() {
    const FILE_NAME: &str = "test_files/1MiB.txt";
    common_quic(current_dir().unwrap(), FILE_NAME).await;
  }

  #[tokio::test]
  async fn ten_paragraphs_quic_test() {
    const FILE_NAME: &str = "test_files/lorem_10_paragraphs.txt";
    common_quic(current_dir().unwrap(), FILE_NAME).await;
  }

  #[tokio::test]
  #[ignore]
  async fn hundred_mib_quic_test() {
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = temp_dir().join(&file_name);
    let _cleanup = FileCleanup::new(&file_path);
    generate_test_file((100 * 2u64.pow(20)) as usize, &file_path).await;
    common_quic(temp_dir(), &file_name).await;
  }

  #[tokio::test]
  #[ignore]
  async fn one_gib_quic_test() {
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = temp_dir().join(&file_name);
    let _cleanup = FileCleanup::new(&file_path);
    generate_test_file(2u64.pow(30) as usize, &file_path).await;
    common_quic(temp_dir(), &file_name).await;
  }

  #[tokio::test]
  #[ignore]
  async fn five_gib_quic_test() {
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = temp_dir().join(&file_name);
    let _cleanup = FileCleanup::new(&file_path);
    generate_test_file((5 * 2u64.pow(30)) as usize, &file_path).await;
    common_quic(temp_dir(), &file_name).await;
  }

  #[tokio::test]
  async fn two_kib_quic_quinn_test() {
    const FILE_NAME: &str = "test_files/2KiB.txt";
    common_quic_quinn(current_dir().unwrap(), FILE_NAME).await;
  }

  #[tokio::test]
  async fn one_mib_quic_quinn_test() {
    const FILE_NAME: &str = "test_files/1MiB.txt";
    common_quic_quinn(current_dir().unwrap(), FILE_NAME).await;
  }

  #[tokio::test]
  async fn ten_paragraphs_quic_quinn_test() {
    const FILE_NAME: &str = "test_files/lorem_10_paragraphs.txt";
    common_quic_quinn(current_dir().unwrap(), FILE_NAME).await;
  }

  #[tokio::test]
  #[ignore]
  async fn hundred_mib_quic_quinn_test() {
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = temp_dir().join(&file_name);
    let _cleanup = FileCleanup::new(&file_path);
    generate_test_file((200 * 2u64.pow(20)) as usize, &file_path).await;
    common_quic_quinn(temp_dir(), &file_name).await;
  }

  #[tokio::test]
  #[ignore]
  async fn one_gib_quic_quinn_test() {
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = temp_dir().join(&file_name);
    let _cleanup = FileCleanup::new(&file_path);
    generate_test_file(2u64.pow(30) as usize, &file_path).await;
    common_quic_quinn(temp_dir(), &file_name).await;
  }

  #[tokio::test]
  #[ignore]
  async fn five_gib_quic_quinn_test() {
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = temp_dir().join(&file_name);
    let _cleanup = FileCleanup::new(&file_path);
    generate_test_file((5 * 2u64.pow(30)) as usize, &file_path).await;
    common_quic_quinn(temp_dir(), &file_name).await;
  }

  #[tokio::test]
  async fn two_kib_quinn_quinn_test() {
    const FILE_NAME: &str = "test_files/2KiB.txt";
    common_quinn_quinn(current_dir().unwrap(), FILE_NAME).await;
  }

  #[tokio::test]
  async fn one_mib_quinn_quinn_test() {
    const FILE_NAME: &str = "test_files/1MiB.txt";
    common_quinn_quinn(current_dir().unwrap(), FILE_NAME).await;
  }

  #[tokio::test]
  async fn ten_paragraphs_quinn_quinn_test() {
    const FILE_NAME: &str = "test_files/lorem_10_paragraphs.txt";
    common_quinn_quinn(current_dir().unwrap(), FILE_NAME).await;
  }

  #[tokio::test]
  #[ignore]
  async fn hundred_mib_quinn_quinn_test() {
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = temp_dir().join(&file_name);
    let _cleanup = FileCleanup::new(&file_path);
    generate_test_file((200 * 2u64.pow(20)) as usize, &file_path).await;
    common_quinn_quinn(temp_dir(), &file_name).await;
  }

  #[tokio::test]
  #[ignore]
  async fn one_gib_quinn_quinn_test() {
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = temp_dir().join(&file_name);
    let _cleanup = FileCleanup::new(&file_path);
    generate_test_file(2u64.pow(30) as usize, &file_path).await;
    common_quinn_quinn(temp_dir(), &file_name).await;
  }

  #[tokio::test]
  #[ignore]
  async fn five_gib_quinn_quinn_test() {
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = temp_dir().join(&file_name);
    let _cleanup = FileCleanup::new(&file_path);
    generate_test_file((5 * 2u64.pow(30)) as usize, &file_path).await;
    common_quinn_quinn(temp_dir(), &file_name).await;
  }

  #[tokio::test]
  async fn not_logged_in_test() {
    let command = Command::new(Commands::Retr, "NONEXISTENT");

    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .build()
      .expect("Settings should be valid");

    let command_processor = setup_test_command_processor_custom(&settings);
    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::NotLoggedIn, None).await;
  }

  #[tokio::test]
  async fn data_channel_not_open_test() {
    let (settings, command_processor) = setup_test_command_processor();

    let command = Command::new(
      Commands::Retr,
      format!("{}/test_files/1MiB.txt", settings.label.clone()),
    );
    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(ACQUIRE_TIMEOUT + 5),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::BadSequenceOfCommands, None).await;
  }

  #[tokio::test]
  async fn no_file_specified_test() {
    let (_, mut command_processor) = setup_test_command_processor();

    let _ = open_tcp_data_channel(&mut command_processor).await;
    let command = Command::new(Commands::Retr, "");
    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
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
    let (settings, mut command_processor) = setup_test_command_processor();

    let _ = open_tcp_data_channel(&mut command_processor).await;
    let command = Command::new(
      Commands::Retr,
      format!("{}/test_files/NONEXISTENT", settings.label.clone()),
    );
    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::FileUnavailable, None).await;
  }

  #[tokio::test]
  async fn no_read_permission_test() {
    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap())
      .permissions(HashSet::new())
      .build()
      .expect("Settings should be valid");

    let mut command_processor = setup_test_command_processor_custom(&settings);

    let _ = open_tcp_data_channel(&mut command_processor).await;
    let command = Command::new(
      Commands::Retr,
      format!("{}/test_files/2KiB.txt", settings.label.clone()),
    );
    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .expect("Command timed out!");

    receive_and_verify_reply(2, &mut rx, ReplyCode::FileUnavailable, None).await;
  }

  #[tokio::test]
  async fn folder_specified_test() {
    let (settings, mut command_processor) = setup_test_command_processor();

    let _ = open_tcp_data_channel(&mut command_processor).await;
    let command = Command::new(
      Commands::Retr,
      format!("{}/test_files", settings.label.clone()),
    );
    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    timeout(
      Duration::from_secs(5),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
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
