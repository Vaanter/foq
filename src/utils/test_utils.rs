use async_trait::async_trait;
use derive_builder::Builder;
use rustls::client::{ServerCertVerified, ServerCertVerifier};
use rustls::{Certificate, ClientConfig, KeyLogFile, ServerConfig, ServerName};
use s2n_quic::provider::io::tokio::Builder as IoBuilder;
use s2n_quic::provider::tls::rustls::Client as TlsClient;
use s2n_quic::Client;
use std::collections::HashSet;
use std::fs::{remove_dir_all, remove_file, OpenOptions as OpenOptionsStd};
use std::io;
use std::io::Error;
use std::iter::Iterator;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use strum::IntoEnumIterator;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::TcpStream;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::Level;

use crate::auth::auth_error::AuthError;
use crate::auth::auth_provider::AuthProvider;
use crate::auth::data_source::DataSource;
use crate::auth::login_form::LoginForm;
use crate::auth::user_data::UserData;
use crate::auth::user_permission::UserPermission;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::data_channels::data_channel_wrapper::DataChannelWrapper;
use crate::data_channels::standard_data_channel_wrapper::StandardDataChannelWrapper;
use crate::global_context::{CERTS, KEY};
use crate::handlers::connection_handler::ConnectionHandler;
use crate::handlers::quic_only_connection_handler::QuicOnlyConnectionHandler;
use crate::handlers::reply_sender::ReplySend;
use crate::io::file_system_view::FileSystemView;
use crate::listeners::quic_only_listener::QuicOnlyListener;
use crate::session::command_processor::CommandProcessor;
use crate::session::session_properties::SessionProperties;

pub(crate) struct TestReplySender {
  tx: Sender<Reply>,
}

impl TestReplySender {
  pub(crate) fn new(tx: Sender<Reply>) -> Self {
    TestReplySender { tx }
  }
}

#[async_trait]
impl ReplySend for TestReplySender {
  async fn send_control_message(&self, reply: Reply) {
    println!(
      "[TestReplySender] Received reply: {}",
      reply.to_string().trim_end()
    );
    self.tx.send(reply).await.unwrap();
  }

  async fn close(&self) -> Result<(), Error> {
    Ok(())
  }
}

#[derive(Clone, Default)]
pub(crate) struct TestDataSource {
  user_data: Vec<UserData>,
}

#[allow(unused)]
impl TestDataSource {
  pub(crate) fn new() -> Self {
    Self::default()
  }

  pub(crate) fn new_with_users(users: Vec<UserData>) -> Self {
    TestDataSource { user_data: users }
  }
}

#[async_trait]
impl DataSource for TestDataSource {
  async fn authenticate(&self, login_form: &LoginForm) -> Result<UserData, AuthError> {
    eprintln!("[TestDataSource] Login attempt with: {:?}", login_form);
    let user = self
      .user_data
      .iter()
      .find(|&u| &u.username == login_form.username.as_ref().unwrap())
      .ok_or(AuthError::UserNotFoundError)?;

    return if &user.password == login_form.password.as_ref().unwrap() {
      Ok(user.clone())
    } else {
      Err(AuthError::UserNotFoundError)
    };
  }
}

pub(crate) struct DirCleanup<'a> {
  directory_path: &'a PathBuf,
}

impl<'a> DirCleanup<'a> {
  pub(crate) fn new(directory_path: &'a PathBuf) -> Self {
    DirCleanup { directory_path }
  }
}

impl<'a> Drop for DirCleanup<'a> {
  fn drop(&mut self) {
    if let Err(remove_result) = remove_dir_all(self.directory_path) {
      eprintln!("Failed to remove directory: {}", remove_result);
    }
  }
}

// Removes the temp file used in tests when dropped
pub(crate) struct FileCleanup<'a>(&'a Path);

impl<'a> FileCleanup<'a> {
  pub(crate) fn new(path: &'a Path) -> Self {
    FileCleanup(path)
  }
}

impl<'a> Drop for FileCleanup<'a> {
  fn drop(&mut self) {
    if let Err(e) = remove_file(self.0) {
      eprintln!("Failed to remove: {:?}, {}", self.0, e);
    };
  }
}

pub(crate) fn create_test_auth_provider(users: Vec<UserData>) -> AuthProvider {
  let mut provider = AuthProvider::new();
  provider.add_data_source(Box::new(TestDataSource::new_with_users(users)));
  provider
}

pub(crate) async fn receive_and_verify_reply(
  time: u64,
  rx: &mut Receiver<Reply>,
  expected: ReplyCode,
  substring: Option<&str>,
) {
  match timeout(Duration::from_secs(time), rx.recv()).await {
    Ok(Some(result)) => {
      assert_eq!(expected, result.code);
      if substring.is_some() {
        assert!(result.to_string().contains(substring.unwrap()));
      }
    }
    Err(_) | Ok(None) => {
      panic!("Failed to receive reply!");
    }
  };
}

pub(crate) async fn receive_and_verify_reply_from_buf<T: AsyncRead + Unpin>(
  time: u64,
  client_reader: &mut BufReader<T>,
  expected: ReplyCode,
  substring: Option<&str>,
) {
  let mut buffer = String::new();
  match timeout(
    Duration::from_secs(time),
    client_reader.read_line(&mut buffer),
  )
  .await
  {
    Ok(Ok(len)) => {
      println!(
        "Received reply from server!: {}. Length: {}",
        buffer.trim(),
        len
      );
      assert!(buffer.trim().starts_with(&(expected as u32).to_string()));
      assert!(buffer.trim().contains(substring.unwrap_or("")));
    }
    Ok(Err(e)) => {
      panic!("Failed to read reply! {}", e);
    }
    Err(_) => panic!("Timeout reading reply message!"),
  }
}

pub(crate) struct NoCertificateVerification {}

impl NoCertificateVerification {
  pub(crate) fn new() -> Self {
    NoCertificateVerification {}
  }
}

#[allow(unused)]
impl ServerCertVerifier for NoCertificateVerification {
  fn verify_server_cert(
    &self,
    end_entity: &Certificate,
    intermediates: &[Certificate],
    server_name: &ServerName,
    scts: &mut dyn Iterator<Item = &[u8]>,
    ocsp_response: &[u8],
    now: SystemTime,
  ) -> Result<ServerCertVerified, rustls::Error> {
    Ok(ServerCertVerified::assertion())
  }
}

pub(crate) fn create_test_client_config() -> ClientConfig {
  ClientConfig::builder()
    .with_safe_defaults()
    .with_custom_certificate_verifier(Arc::new(NoCertificateVerification::new()))
    .with_no_client_auth()
}

pub(crate) fn create_test_server_config() -> ServerConfig {
  ServerConfig::builder()
    .with_safe_defaults()
    .with_no_client_auth()
    .with_single_cert(CERTS.clone(), KEY.clone())
    .unwrap()
}

pub(crate) const LOCALHOST: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

pub(crate) async fn open_tcp_data_channel(command_processor: &mut CommandProcessor) -> TcpStream {
  let addr = match command_processor.data_wrapper.open_data_stream().await {
    Ok(addr) => addr,
    Err(_) => panic!("Failed to open passive data listener!"),
  };

  println!("Connecting to passive listener");
  let client_dc = match TcpStream::connect(addr).await {
    Ok(c) => c,
    Err(e) => {
      panic!("Client passive connection failed: {}", e);
    }
  };
  println!("Client passive connection successful!");

  client_dc
}

pub(crate) fn setup_test_command_processor() -> (CommandProcessorSettings, CommandProcessor) {
  let settings = CommandProcessorSettingsBuilder::default()
    .username(Some("testuser".to_string()))
    .build()
    .unwrap();
  let command_processor = setup_test_command_processor_custom(&settings);
  (settings, command_processor)
}

pub(crate) fn setup_test_command_processor_custom(
  settings: &CommandProcessorSettings,
) -> CommandProcessor {
  let mut session_properties = SessionProperties::new();
  if let Some(username) = &settings.username {
    let view = FileSystemView::new(
      settings.view_root.clone(),
      settings.label.clone(),
      settings.permissions.clone(),
    );
    session_properties
      .file_system_view_root
      .set_views(vec![view]);
    session_properties.username.replace(username.clone());
  }

  if let Some(change_path) = &settings.change_path {
    session_properties
      .file_system_view_root
      .change_working_directory(change_path)
      .expect("change_path should be valid");
  }

  let session_properties = Arc::new(RwLock::new(session_properties));
  let wrapper = Arc::new(StandardDataChannelWrapper::new(LOCALHOST));
  CommandProcessor::new(session_properties, wrapper)
}

#[derive(Builder, Default, Clone)]
#[builder(pattern = "owned")]
pub(crate) struct CommandProcessorSettings {
  #[builder(default = "String::from(\"test\")")]
  pub(crate) label: String,
  #[builder(default = "std::env::current_dir().unwrap()")]
  pub(crate) view_root: PathBuf,
  #[builder(default = "UserPermission::iter().collect()")]
  pub(crate) permissions: HashSet<UserPermission>,
  #[builder(default)]
  pub(crate) username: Option<String>,
  #[builder(default)]
  pub(crate) change_path: Option<String>,
}

pub(crate) async fn run_quic_listener(
  ct: CancellationToken,
  server_addr: SocketAddr,
) -> (JoinHandle<()>, SocketAddr) {
  let mut listener = QuicOnlyListener::new(server_addr).unwrap();
  let addr = listener.server.local_addr().unwrap();
  let handler_fut = tokio::spawn(async move {
    let conn = listener.accept(ct.clone()).await.unwrap();
    let mut handler = QuicOnlyConnectionHandler::new(conn);

    handler
      .handle(ct)
      .await
      .expect("Handler should exit gracefully");
  });
  (handler_fut, addr)
}

pub(crate) fn create_tls_client_config(alpn: &str) -> ClientConfig {
  let mut client_config = ClientConfig::builder()
    .with_safe_defaults()
    .with_custom_certificate_verifier(Arc::new(NoCertificateVerification::new()))
    .with_no_client_auth();
  client_config.alpn_protocols = vec![alpn.as_bytes().to_vec()];
  client_config
}

pub(crate) async fn generate_test_file(amount: usize, output_file: &Path) {
  println!(
    "Generating test file, size: {}B, path: {:?}",
    amount,
    output_file.as_os_str()
  );
  const MAX_CHUNK_SIZE: usize = 32864;
  let file = OpenOptions::new()
    .create(true)
    .write(true)
    .truncate(true)
    .append(false)
    .open(output_file)
    .await
    .unwrap();
  let mut output_writer = BufWriter::with_capacity(MAX_CHUNK_SIZE, file);

  let mut remaining = amount;
  let chunk = vec![0u8; MAX_CHUNK_SIZE];
  while remaining > MAX_CHUNK_SIZE {
    output_writer
      .write_all(&chunk)
      .await
      .expect("Writing chunk to test file should succeed");
    remaining -= MAX_CHUNK_SIZE;
  }

  output_writer
    .write_all(&chunk[..remaining])
    .await
    .expect("Writing chunk to test file should succeed");
  output_writer.flush().await.unwrap();
}

pub(crate) fn setup_transfer_command_processor<T: DataChannelWrapper + 'static>(
  data_channel_wrapper: T,
  root: PathBuf,
) -> CommandProcessor {
  println!("Running setup.");

  let label = "test_files".to_string();

  let settings = CommandProcessorSettingsBuilder::default()
    .label(label.clone())
    .change_path(Some(label.clone()))
    .username(Some("testuser".to_string()))
    .view_root(root)
    .change_path(Some(label.clone()))
    .build()
    .expect("Settings should be valid");

  let mut command_processor = setup_test_command_processor_custom(&settings);
  command_processor.data_wrapper = Arc::new(data_channel_wrapper);
  println!("Setup completed.");
  command_processor
}

pub(crate) fn setup_s2n_client() -> Client {
  let mut tls_config = create_tls_client_config("ftpoq-1");
  tls_config.key_log = Arc::new(KeyLogFile::new());
  let tls_client = TlsClient::new(tls_config);

  let io = IoBuilder::default()
    .with_receive_address(LOCALHOST)
    .unwrap()
    .with_recv_buffer_size(50 * 2usize.pow(20))
    .unwrap()
    .with_internal_recv_buffer_size(50 * 2usize.pow(20))
    .unwrap()
    .build()
    .unwrap();

  Client::builder()
    .with_tls(tls_client)
    .expect("Client requires valid TLS settings!")
    .with_io(io)
    .expect("Client requires valid I/O settings!")
    .start()
    .expect("Client must be able to start")
}

pub(crate) fn setup_tracing() {
  let subscriber = tracing_subscriber::fmt()
    .with_env_filter(format!("foq={}", Level::DEBUG))
    .with_file(true)
    .with_line_number(true)
    .with_thread_ids(true)
    .with_target(false)
    .finish();
  let _ = tracing::subscriber::set_global_default(subscriber);
}

pub(crate) fn touch(path: &Path) -> io::Result<()> {
  OpenOptionsStd::new()
    .create(true)
    .write(true)
    .open(path)
    .map(|_| ())
}
