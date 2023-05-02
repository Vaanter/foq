use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use s2n_quic::{provider::io::tokio::Builder as IoBuilder, Connection, Server};
use tokio::sync::broadcast::Receiver;
use tokio::sync::Mutex;
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
  shutdown_receiver: Receiver<()>,
}

impl QuicOnlyListener {
  pub(crate) fn new(
    addr: SocketAddr,
    shutdown_receiver: Receiver<()>,
  ) -> Result<Self, Box<dyn Error>> {
    let io = IoBuilder::default().with_receive_address(addr)?.build()?;

    let server = Server::builder()
      .with_tls((CERT_PEM, KEY_PEM))?
      .with_io(io)?
      .start()?;

    Ok(QuicOnlyListener {
      server,
      handlers: vec![],
      shutdown_receiver,
    })
  }

  pub(crate) async fn accept(&mut self) -> Result<Connection, anyhow::Error> {
  #[tracing::instrument(skip(self))]
    let value = tokio::select! {
      conn = self.server.accept() => Ok(conn.unwrap()),
      _ = self.shutdown_receiver.recv() => Err(anyhow::anyhow!("Quic listener shutdown!"))
        info!("Quic listener shutdown!");
    };
    value
  }
}
