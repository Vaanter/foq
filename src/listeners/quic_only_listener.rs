use std::error::Error;
use std::net::SocketAddr;

use rustls::ServerConfig;
use s2n_quic::{
  provider::io::tokio::Builder as IoBuilder, provider::tls::default::Server as TlsServer,
  Connection, Server,
};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::global_context::{CERTS, KEY};

pub(crate) struct QuicOnlyListener {
  pub(crate) server: Server,
}

impl QuicOnlyListener {
  pub(crate) fn new(addr: SocketAddr) -> Result<Self, Box<dyn Error>> {
    let io = IoBuilder::default().with_receive_address(addr)?.build()?;

    let certs = CERTS.clone();
    let key = KEY.clone();

    let mut config = ServerConfig::builder()
      .with_safe_defaults()
      .with_no_client_auth()
      .with_single_cert(certs, key)
      .map_err(|err| tokio::io::Error::new(tokio::io::ErrorKind::InvalidInput, err))?;
    config.alpn_protocols = vec!["foq".as_bytes().to_vec()];

    let tls_server = TlsServer::new(config);

    let server = Server::builder()
      .with_tls(tls_server)?
      .with_io(io)?
      .start()?;

    Ok(QuicOnlyListener { server })
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
