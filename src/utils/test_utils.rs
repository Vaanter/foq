use std::collections::HashSet;
use std::env::current_dir;
use std::io::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use rustls::client::{ServerCertVerified, ServerCertVerifier};
use rustls::{Certificate, ClientConfig, ServerConfig, ServerName};
use tokio::io::{
  AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, BufReader, BufWriter,
};
use tokio::net::TcpStream;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use crate::auth::auth_error::AuthError;
use crate::auth::auth_provider::AuthProvider;
use crate::auth::data_source::DataSource;
use crate::auth::login_form::LoginForm;
use crate::auth::user_data::UserData;
use crate::auth::user_permission::UserPermission;
use crate::handlers::connection_handler::ConnectionHandler;
use crate::handlers::quic_only_connection_handler::QuicOnlyConnectionHandler;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::data_channels::standard_data_channel_wrapper::StandardDataChannelWrapper;
use crate::io::file_system_view::FileSystemView;
use crate::listeners::quic_only_listener::QuicOnlyListener;
use crate::session::session_properties::SessionProperties;
use crate::utils::tls_utils::{load_certs, load_keys};

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

  async fn close(&mut self) -> Result<(), Error> {
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

pub static CERT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/certs/server-cert.pem");

pub static KEY_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/certs/server-key.pem");

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
  let cert = load_certs(Path::new(CERT_PATH)).unwrap();
  let mut key = load_keys(Path::new(KEY_PATH)).unwrap();
  ServerConfig::builder()
    .with_safe_defaults()
    .with_no_client_auth()
    .with_single_cert(cert, key.remove(0))
    .unwrap()
}

pub(crate) const LOCALHOST: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

pub(crate) async fn open_tcp_data_channel(command_processor: &mut CommandProcessor) -> TcpStream {
  let addr = match command_processor
    .data_wrapper
    .clone()
    .lock()
    .await
    .open_data_stream()
    .await
  {
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

  let _ = command_processor
    .data_wrapper
    .lock()
    .await
    .get_data_stream()
    .await
    .lock()
    .await;
  client_dc
}

pub(crate) fn setup_test_command_processor() -> (&'static str, CommandProcessor) {
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
  let _ = session_properties.username.insert("test".to_string());

  let session_properties = Arc::new(RwLock::new(session_properties));
  let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
  let command_processor = CommandProcessor::new(session_properties, wrapper);
  (label, command_processor)
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
