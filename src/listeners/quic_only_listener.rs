use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::sync::Arc;

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
  pub(crate) fn new(addr: SocketAddr) -> Result<Self, Error> {
    if addr.is_ipv6() {
      return Err(Error::new(ErrorKind::Unsupported, "IPv6 is not supported!"));
    }

    let io = IoBuilder::default().with_receive_address(addr)?.build()?;

    let certs = CERTS.clone();
    let key = KEY.clone();

    let mut config = ServerConfig::builder()
      .with_safe_defaults()
      .with_no_client_auth()
      .with_single_cert(certs, key)
      .map_err(|err| tokio::io::Error::new(tokio::io::ErrorKind::InvalidInput, err))?;
    config.alpn_protocols = vec!["ftpoq-1".as_bytes().to_vec()];

    if std::env::var_os("SSLKEYLOGFILE").is_some() {
      config.key_log = Arc::new(rustls::KeyLogFile::new());
    }

    let tls_server = TlsServer::new(config);

    let server = Server::builder()
      .with_tls(tls_server)
      .unwrap()
      .with_io(io)
      .unwrap()
      .start()
      .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;

    Ok(QuicOnlyListener { server })
  }

  #[tracing::instrument(skip_all)]
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
