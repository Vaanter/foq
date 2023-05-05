use std::io::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use rustls::client::{ServerCertVerified, ServerCertVerifier};
use rustls::{Certificate, ClientConfig, ServerConfig, ServerName};
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::time::timeout;

use crate::auth::auth_error::AuthError;
use crate::auth::auth_provider::AuthProvider;
use crate::auth::data_source::DataSource;
use crate::auth::login_form::LoginForm;
use crate::auth::user_data::UserData;
use crate::handlers::reply_sender::ReplySend;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;
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
      "TestReplySender: received reply: {}",
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
    eprintln!("Received: {:?}", login_form);
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

#[cfg(test)]
pub(crate) fn create_test_client_config() -> ClientConfig {
  ClientConfig::builder()
    .with_safe_defaults()
    .with_custom_certificate_verifier(Arc::new(NoCertificateVerification::new()))
    .with_no_client_auth()
}

#[cfg(test)]
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
