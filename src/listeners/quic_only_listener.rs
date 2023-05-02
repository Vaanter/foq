use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use s2n_quic::{provider::io::tokio::Builder as IoBuilder, Connection, Server};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::handlers::quic_only_connection_handler::QuicOnlyConnectionHandler;

/// NOTE: this certificate is to be used for demonstration purposes only!
pub static CERT_PEM: &str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/certs/server-cert.pem"
));
/// NOTE: this certificate is to be used for demonstration purposes only!
pub static KEY_PEM: &str =
  include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/certs/server-key.pem"));

pub(crate) struct QuicOnlyListener {
  pub(crate) server: Server,
  pub(crate) handlers: Vec<Arc<Mutex<QuicOnlyConnectionHandler>>>,
}

impl QuicOnlyListener {
  pub(crate) fn new(addr: SocketAddr) -> Result<Self, Box<dyn Error>> {
    let io = IoBuilder::default().with_receive_address(addr)?.build()?;

    let server = Server::builder()
      .with_tls((CERT_PEM, KEY_PEM))?
      .with_io(io)?
      .start()?;

    Ok(QuicOnlyListener {
      server,
      handlers: vec![],
    })
  }

  #[tracing::instrument(skip(self))]
  pub(crate) async fn accept(&mut self, token: CancellationToken) -> Option<Connection> {
    let value = tokio::select! {
      conn = self.server.accept() => Some(conn.unwrap()),
      _ = token.cancelled() => {
        info!("Quic listener shutdown!");
        None
      }
    };
    value
  }
}
