use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use rustls::ServerConfig;
use s2n_quic::provider::congestion_controller::Bbr;
use s2n_quic::provider::limits::{Limits, Provider};
use s2n_quic::{
  provider::io::tokio::Builder as IoBuilder, provider::tls::rustls::Server as TlsServer,
  Connection, Server,
};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::global_context::{CERTS, KEY};

pub(crate) struct QuicOnlyListener {
  pub(crate) server: Server,
}

impl QuicOnlyListener {
  /// Construct a new QUIC listener listening on specified [`SocketAddr`].
  ///
  /// # TLS config
  /// TLS is backed by [`rustls`]. Configuration uses default settings, and the ALPN is set to
  /// 'ftpoq-1'. If the 'SSLKEYLOGFILE' environment variable is set, then the session secrets are
  /// exported.
  ///
  /// # Failure points
  /// Constructing the listener will fail under these circumstances:
  /// - IPv6 address is specified
  /// - The certificate and key are not set or invalid
  /// - Binding to the IP address fails (e.g.: the port is in use)
  ///
  ///
  pub(crate) fn new(addr: SocketAddr) -> Result<Self, Error> {
    if addr.is_ipv6() {
      return Err(Error::new(ErrorKind::Unsupported, "IPv6 is not supported!"));
    }

    let io = IoBuilder::default()
      .with_receive_address(addr)?
      .with_send_buffer_size(50 * 2usize.pow(20))?
      .with_recv_buffer_size(50 * 2usize.pow(20))?
      .with_internal_send_buffer_size(50 * 2usize.pow(20))?
      .with_internal_recv_buffer_size(50 * 2usize.pow(20))?
      .build()?;
    let limits = Limits::new()
      .with_max_idle_timeout(Duration::from_secs(30))
      .map_err(|e| Error::new(ErrorKind::Other, e))?
      .start()
      .unwrap();

    let congestion_controller = Bbr::default();

    let certs = CERTS.clone();
    let key = KEY.clone();

    let mut config = ServerConfig::builder()
      .with_safe_defaults()
      .with_no_client_auth()
      .with_single_cert(certs, key)
      .map_err(|err| Error::new(ErrorKind::InvalidInput, err))?;
    config.alpn_protocols = vec!["ftpoq-1".as_bytes().to_vec()];

    if std::env::var_os("SSLKEYLOGFILE").is_some() {
      config.key_log = Arc::new(rustls::KeyLogFile::new());
    }

    let tls_server = TlsServer::new(config);

    let server = Server::builder()
      .with_congestion_controller(congestion_controller)
      .unwrap()
      .with_tls(tls_server)
      .unwrap()
      .with_io(io)
      .unwrap()
      .with_limits(limits)
      .unwrap()
      .start()
      .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;

    Ok(QuicOnlyListener { server })
  }

  /// Accepts a connection from the client.
  ///
  /// This function awaits a connections from client. If the [`CancellationToken`] is triggered,
  /// this will return [`Option::None`], otherwise it will contain the created connection.
  ///
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
