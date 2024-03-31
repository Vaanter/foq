use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::select;
use tracing::{debug, info, warn};

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::r#impl::shared::{
  acquire_data_channel, get_open_file_result, get_transfer_reply, transfer_data,
};
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::io::open_options_flags::OpenOptionsWrapperBuilder;
use crate::session::command_processor::CommandProcessor;

pub(crate) async fn stor(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  debug_assert_eq!(command.command, Commands::Stor);

  if command.argument.is_empty() {
    reply_sender
      .send_control_message(Reply::new(
        ReplyCode::SyntaxErrorInParametersOrArguments,
        "No file specified!",
      ))
      .await;
    return;
  }

  let session_properties = command_processor.session_properties.read().await;

  if !session_properties.is_logged_in() {
    reply_sender
      .send_control_message(Reply::new(ReplyCode::NotLoggedIn, "User not logged in!"))
      .await;
    return;
  }

  let data_channel_pair = acquire_data_channel(command_processor.data_wrapper.clone()).await;
  let (mut data_channel, token) = match data_channel_pair {
    Ok((dc, token)) => (dc, token),
    Err(e) => {
      return reply_sender.send_control_message(e).await;
    }
  };

  let options = OpenOptionsWrapperBuilder::default()
    .write(true)
    .truncate(true)
    .create(true)
    .build()
    .unwrap();
  info!(
    "User '{}' opening file '{}'.",
    session_properties.username.as_ref().unwrap(),
    &command.argument
  );
  let file = session_properties
    .file_system_view_root
    .open_file(&command.argument, options)
    .await;

  let mut file = match get_open_file_result(file) {
    Ok(f) => f,
    Err(reply) => {
      reply_sender.send_control_message(reply).await;
      return;
    }
  };

  reply_sender
    .send_control_message(Reply::new(
      ReplyCode::FileStatusOkay,
      "Starting file transfer!",
    ))
    .await;

  debug!("Receiving file data!");
  let mut buffer = vec![0; 65536];
  let transfer = transfer_data(&mut data_channel, &mut file, &mut buffer);

  let success = select! {
    result = transfer => result,
    _ = token.cancelled() => {
      debug!("Received transfer abort");
      false
    }
  };
  if let Err(e) = file.sync_data().await {
    warn!("Failed to sync file data! {e}");
  };

  reply_sender
    .send_control_message(get_transfer_reply(success))
    .await;

  if success {
    if let Err(e) = data_channel.shutdown().await {
      warn!("Failed to shutdown data channel after writing! {e}");
    }
  }
}

#[cfg(test)]
mod tests {
  use std::collections::HashSet;
  use std::env::{current_dir, temp_dir};
  use std::path::Path;
  use std::sync::Arc;
  use std::time::Duration;

  use blake3::Hasher;
  use quinn::TransportConfig;
  use rustls::KeyLogFile;
  use s2n_quic::client::Connect;
  use tokio::fs::OpenOptions;
  use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
  use tokio::sync::mpsc::channel;
  use tokio::sync::{Mutex, RwLock};
  use tokio::time::timeout;
  use tokio_util::sync::CancellationToken;
  use uuid::Uuid;

  use crate::auth::user_permission::UserPermission;
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::r#impl::shared::ACQUIRE_TIMEOUT;
  use crate::commands::reply_code::ReplyCode;
  use crate::data_channels::quic_only_data_channel_wrapper::QuicOnlyDataChannelWrapper;
  use crate::data_channels::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::file_system_view::FileSystemView;
  use crate::listeners::quic_only_listener::QuicOnlyListener;
  use crate::session::command_processor::CommandProcessor;
  use crate::session::protection_mode::ProtMode;
  use crate::session::session_properties::SessionProperties;
  use crate::utils::test_utils::*;

  const TIMEOUT_SECS: u64 = 600;

  async fn common(local_file: &Path, remote_file: &str) {
    assert!(
      local_file.exists(),
      "Test file does not exist! Cannot proceed!"
    );

    let remote_file_path = temp_dir().join(remote_file);
    println!("Remote file: {:?}", &remote_file_path);

    let _cleanup = FileCleanup::new(&remote_file_path);

    let command = Command::new(Commands::Stor, remote_file);

    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(temp_dir())
      .build()
      .expect("Settings should be valid");

    let mut command_processor = setup_test_command_processor_custom(&settings);

    let mut client_dc = open_tcp_data_channel(&mut command_processor).await;

    let transfer = transfer(
      local_file,
      remote_file,
      command,
      command_processor,
      &mut client_dc,
    );

    match timeout(Duration::from_secs(TIMEOUT_SECS), transfer).await {
      Ok(()) => println!("Transfer complete!"),
      Err(_) => panic!("Transfer timed out!"),
    }
  }

  async fn common_quic(local_file: &Path, remote_file: &str) {
    assert!(
      local_file.exists(),
      "Test file does not exist! Cannot proceed!"
    );

    let remote_file_path = temp_dir().join(remote_file);
    println!("Remote file: {:?}", &remote_file_path);

    let _cleanup = FileCleanup::new(&remote_file_path);

    let mut listener = QuicOnlyListener::new(LOCALHOST).unwrap();
    let addr = listener.server.local_addr().unwrap();
    let token = CancellationToken::new();
    let command = Command::new(Commands::Stor, remote_file);
    let test_handle = tokio::spawn(async move {
      let connection = listener.accept(token.clone()).await.unwrap();
      let wrapper = QuicOnlyDataChannelWrapper::new(LOCALHOST, Arc::new(Mutex::new(connection)));
      setup_transfer_command_processor(wrapper, temp_dir())
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

    let transfer = transfer(
      local_file,
      remote_file,
      command,
      command_processor,
      client_dc,
    );

    match timeout(Duration::from_secs(TIMEOUT_SECS), transfer).await {
      Ok(()) => println!("Transfer complete!"),
      Err(_) => panic!("Transfer timed out!"),
    }
  }

  async fn common_quic_quinn(local_file: &Path, remote_file: &str) {
    assert!(
      local_file.exists(),
      "Test file does not exist! Cannot proceed!"
    );

    let remote_file_path = temp_dir().join(remote_file);
    println!("Remote file: {:?}", &remote_file_path);

    let _cleanup = FileCleanup::new(&remote_file_path);

    let mut listener = QuicOnlyListener::new(LOCALHOST).unwrap();
    let addr = listener.server.local_addr().unwrap();
    let command = Command::new(Commands::Stor, remote_file);
    let token = CancellationToken::new();
    let test_handle = tokio::spawn(async move {
      let connection = listener.accept(token.clone()).await.unwrap();
      let wrapper = QuicOnlyDataChannelWrapper::new(LOCALHOST, Arc::new(Mutex::new(connection)));
      setup_transfer_command_processor(wrapper, temp_dir())
    });

    let mut quinn_client = quinn::Endpoint::client(LOCALHOST).unwrap();

    let mut tls_config = create_tls_client_config("ftpoq-1");
    tls_config.key_log = Arc::new(KeyLogFile::new());
    let mut transport_config = TransportConfig::default();
    transport_config.keep_alive_interval(Some(Duration::from_secs(10)));
    let mut client_config = quinn::ClientConfig::new(Arc::new(tls_config));
    client_config.transport_config(Arc::new(transport_config));
    quinn_client.set_default_client_config(client_config);

    let connection = match quinn_client.connect(addr, "localhost").unwrap().await {
      Ok(conn) => conn,
      Err(e) => {
        panic!("Client failed to connect to the server! {}", e);
      }
    };

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

    let (client_dc_send, _) = match connection.open_bi().await {
      Ok(client_dc) => client_dc,
      Err(e) => panic!("Failed to open data channel! Error: {}", e),
    };

    transfer(
      local_file,
      remote_file,
      command,
      command_processor,
      client_dc_send,
    )
    .await;
    //println!("stats: {:#?}", connection);
  }

  async fn transfer<T: AsyncWrite + Unpin>(
    local_file: &Path,
    remote_file: &str,
    command: Command,
    command_processor: CommandProcessor,
    mut client_dc: T,
  ) {
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

    let mut sender_file_hasher = Hasher::new();

    let transfer = transfer_to(local_file, &mut client_dc, &mut sender_file_hasher);

    match timeout(Duration::from_secs(TIMEOUT_SECS), transfer).await {
      Ok(n) => println!("Transfer complete, send: {} bytes!", n),
      Err(_) => panic!("Transfer timed out!"),
    }

    receive_and_verify_reply(2, &mut rx, ReplyCode::ClosingDataConnection, None).await;

    command_fut.await.expect("Command should complete!");

    verify_transfer(remote_file, &sender_file_hasher).await;
  }

  async fn transfer_to<T: AsyncWrite + Unpin>(send_file: &Path, client_dc: &mut T, sender_file_hasher: &mut Hasher) -> usize {
    let mut sends = 0;
    let mut send_file = OpenOptions::new()
      .read(true)
      .open(send_file)
      .await
      .expect("Send test file must exist!");

    const SENDER_BUFFER_SIZE: usize = 16384;
    let mut sender_buffer = [0; SENDER_BUFFER_SIZE];

    loop {
      let sent_bytes = match send_file.read(&mut sender_buffer).await {
        Ok(len) => len,
        Err(e) => panic!("Failed to read local file! {e}"),
      };

      if sent_bytes == 0 {
        break;
      }

      if let Err(e) = client_dc.write_all(&sender_buffer[..sent_bytes]).await {
        panic!("File transfer failed! {e}");
      };
      if let Err(e) = client_dc.flush().await {
        eprintln!("Failed to flush data, {e}");
      };
      sends += sent_bytes;

      sender_file_hasher.update(&sender_buffer[..sent_bytes]);

      sender_buffer.fill(0);
    }

    if let Err(e) = client_dc.shutdown().await {
      eprintln!("Failed to shutdown client data channels, {e}");
    }
    sends
  }

  async fn verify_transfer(
    remote_file: &str,
    sender_file_hasher: &Hasher,
  ) {
    let mut receiver_file_hasher = Hasher::new();

    const RECEIVER_BUFFER_SIZE: usize = 16384;
    let mut receiver_buffer = [0; RECEIVER_BUFFER_SIZE];

    let remote = temp_dir().join(remote_file);
    let mut remote_file = OpenOptions::new()
      .read(true)
      .open(remote)
      .await
      .expect("Remote test file must exist!");

    loop {
      let received_bytes = match remote_file.read(&mut receiver_buffer).await {
        Ok(len) => len,
        Err(e) => panic!("Failed to read remote file! {e}"),
      };

      if received_bytes == 0 {
        break;
      }
      receiver_file_hasher.update(&receiver_buffer[..received_bytes]);
      receiver_buffer.fill(0);
    }

    assert_eq!(
      sender_file_hasher.finalize(),
      receiver_file_hasher.finalize(),
      "File hashes do not match!"
    );
    println!("File hashes match!");
  }

  #[tokio::test]
  async fn two_kib_test() {
    const LOCAL_FILE: &str = "test_files/2KiB.txt";
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common(Path::new(LOCAL_FILE), &remote_file).await;
  }

  #[tokio::test]
  async fn one_mib_test() {
    const LOCAL_FILE: &str = "test_files/1MiB.txt";
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common(Path::new(LOCAL_FILE), &remote_file).await;
  }

  #[tokio::test]
  #[ignore]
  async fn one_gib_test() {
    let file_path = temp_dir().join(format!("{}.test", Uuid::new_v4().as_hyphenated()));
    let _cleanup = FileCleanup::new(&file_path);
    generate_test_file(2u64.pow(30) as usize, &file_path).await;
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common(&file_path, &remote_file).await;
  }

  #[tokio::test]
  #[ignore]
  async fn five_gib_test() {
    let file_path = temp_dir().join(format!("{}.test", Uuid::new_v4().as_hyphenated()));
    let _cleanup = FileCleanup::new(&file_path);
    generate_test_file((5 * 2u64.pow(30)) as usize, &file_path).await;
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common(&file_path, &remote_file).await;
  }

  #[tokio::test]
  async fn ten_paragraphs_test() {
    const LOCAL_FILE: &str = "test_files/lorem_10_paragraphs.txt";
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common(Path::new(LOCAL_FILE), &remote_file).await;
  }

  #[tokio::test]
  async fn two_kib_quic_test() {
    const LOCAL_FILE: &str = "test_files/2KiB.txt";
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common_quic(Path::new(LOCAL_FILE), &remote_file).await;
  }

  #[tokio::test]
  async fn one_mib_quic_test() {
    const LOCAL_FILE: &str = "test_files/1MiB.txt";
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common_quic(Path::new(LOCAL_FILE), &remote_file).await;
  }

  #[tokio::test]
  #[ignore]
  async fn one_gib_quic_test() {
    let file_path = temp_dir().join(format!("{}.test", Uuid::new_v4().as_hyphenated()));
    let _cleanup = FileCleanup::new(&file_path);
    generate_test_file(2u64.pow(30) as usize, Path::new(&file_path)).await;
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common_quic(&file_path, &remote_file).await;
  }

  #[tokio::test]
  #[ignore]
  async fn five_gib_quic_test() {
    let file_path = temp_dir().join(format!("{}.test", Uuid::new_v4().as_hyphenated()));
    let _cleanup = FileCleanup::new(&file_path);
    generate_test_file((5 * 2u64.pow(30)) as usize, Path::new(&file_path)).await;
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common_quic(&file_path, &remote_file).await;
  }

  #[tokio::test]
  async fn ten_paragraphs_quic_test() {
    const LOCAL_FILE: &str = "test_files/lorem_10_paragraphs.txt";
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common_quic(Path::new(LOCAL_FILE), &remote_file).await;
  }

  #[tokio::test]
  async fn two_kib_quic_quinn_test() {
    const LOCAL_FILE: &str = "test_files/2KiB.txt";
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common_quic_quinn(Path::new(LOCAL_FILE), &remote_file).await;
  }

  #[tokio::test]
  async fn one_mib_quic_quinn_test() {
    const LOCAL_FILE: &str = "test_files/1MiB.txt";
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common_quic_quinn(Path::new(LOCAL_FILE), &remote_file).await;
  }

  #[tokio::test]
  #[ignore]
  async fn one_gib_quic_quinn_test() {
    let file_path = temp_dir().join(format!("{}.test", Uuid::new_v4().as_hyphenated()));
    let _cleanup = FileCleanup::new(&file_path);
    generate_test_file(2u64.pow(30) as usize, &file_path).await;
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common_quic_quinn(&file_path, &remote_file).await;
  }

  #[tokio::test]
  #[ignore]
  async fn five_gib_quic_quinn_test() {
    let file_path = temp_dir().join(format!("{}.test", Uuid::new_v4().as_hyphenated()));
    let _cleanup = FileCleanup::new(&file_path);
    generate_test_file((5 * 2u64.pow(30)) as usize, &file_path).await;
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common_quic_quinn(&file_path, &remote_file).await;
  }

  #[tokio::test]
  async fn ten_paragraphs_quic_quinn_test() {
    const LOCAL_FILE: &str = "test_files/lorem_10_paragraphs.txt";
    let remote_file = format!("{}.test", Uuid::new_v4().as_hyphenated());
    common_quic_quinn(Path::new(LOCAL_FILE), &remote_file).await;
  }

  #[tokio::test]
  async fn not_logged_in_test() {
    let wrapper = Arc::new(StandardDataChannelWrapper::new(LOCALHOST));
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let _ = open_tcp_data_channel(&mut command_processor).await;

    let command = Command::new(Commands::Stor, "NONEXISTENT");
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
    let wrapper = Arc::new(StandardDataChannelWrapper::new(LOCALHOST));

    let label = "test";
    let view = FileSystemView::new(
      current_dir().unwrap(),
      label,
      HashSet::from([
        UserPermission::Read,
        UserPermission::Write,
        UserPermission::Create,
      ]),
    );

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    session_properties
      .write()
      .await
      .file_system_view_root
      .set_views(vec![view]);
    let _ = session_properties
      .write()
      .await
      .username
      .insert("test".to_string());
    let command_processor = CommandProcessor::new(session_properties, wrapper);

    let command = Command::new(Commands::Stor, "NONEXISTENT".to_string());
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
    let wrapper = Arc::new(StandardDataChannelWrapper::new(LOCALHOST));

    let label = "test";
    let view = FileSystemView::new(
      current_dir().unwrap(),
      label,
      HashSet::from([
        UserPermission::Read,
        UserPermission::Write,
        UserPermission::Create,
      ]),
    );

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    session_properties
      .write()
      .await
      .file_system_view_root
      .set_views(vec![view]);
    let _ = session_properties
      .write()
      .await
      .username
      .insert("test".to_string());
    let mut command_processor = CommandProcessor::new(session_properties, wrapper);

    let _ = open_tcp_data_channel(&mut command_processor).await;

    let command = Command::new(Commands::Stor, "");
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
