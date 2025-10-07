use crate::global_context::TLS_CONFIG;
use anyhow::{Error, bail};
use quinn::crypto::rustls::QuicServerConfig;
use quinn::{Endpoint, Incoming, ServerConfig, TransportConfig};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub(crate) struct QuinnListener {
  pub(crate) listener: Endpoint,
}

impl QuinnListener {
  /// Constructs a new QUIC listener listening on specified [`SocketAddr`].
  ///
  /// # TLS config
  /// Uses TLS settings from the global context
  ///
  /// # Failure points
  /// Constructing the listener will fail in the following cases:
  /// - IPv6 address is specified
  /// - The TLS config is not available
  /// - Quinn server config cannot be created from the TLS config
  /// - The listener fails to bind to the address
  ///
  pub(crate) fn new(addr: SocketAddr) -> Result<Self, Error> {
    if addr.is_ipv6() {
      bail!("IPv6 is not supported!");
    }
    let tls_config = match TLS_CONFIG.clone() {
      Some(tls) => tls,
      None => {
        bail!("TLS not available, unable to create Quinn listener!");
      }
    };
    let mut config = ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(tls_config)?));
    let mut transport_config = TransportConfig::default();
    transport_config.keep_alive_interval(Some(Duration::from_secs(5)));
    config.transport = Arc::new(transport_config);

    Ok(QuinnListener {
      listener: Endpoint::server(config, addr)?,
    })
  }

  /// Listens for new connection from client.
  ///
  /// Awaits until a client initiates a connection or until the [`CancellationToken`] is triggered.
  /// Returns an [`Option`] with the [`Incoming`] connection or [`None`] if ```token``` was triggered
  /// or the listener is closed.
  #[tracing::instrument(skip_all)]
  pub(crate) async fn accept(&self, token: CancellationToken) -> Option<Incoming> {
    let value = tokio::select! {
      conn = self.listener.accept() => conn,
      _ = token.cancelled() => {
        info!("Quinn listener shutdown!");
        None
      }
    };
    value
  }
}
