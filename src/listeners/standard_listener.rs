use std::io::{Error, ErrorKind};
use std::net::SocketAddr;

use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;
use tracing::info;

pub(crate) struct StandardListener {
  pub(crate) listener: TcpListener,
}

impl StandardListener {
  /// Construct a new TCP listener, listening on specified [`SocketAddr`].
  ///
  /// # Failure points
  /// Constructing the listener will fail under these circumstances:
  /// - IPv6 address is specified
  /// - Binding to the IP address fails (e.g.: the port is in use)
  ///
  pub(crate) async fn new(addr: SocketAddr) -> Result<Self, Error> {
    if addr.is_ipv6() {
      return Err(Error::new(ErrorKind::Unsupported, "IPv6 is not supported!"));
    }
    Ok(StandardListener {
      listener: TcpListener::bind(addr).await?,
    })
  }

  /// Accepts a connection from the client.
  ///
  /// This function awaits a connections from client. If the [`CancellationToken`] is triggered,
  /// this will return [`Option::None`], otherwise it will contain the created connection.
  ///
  #[tracing::instrument(skip_all)]
  pub(crate) async fn accept(
    &mut self,
    token: CancellationToken,
  ) -> Option<(TcpStream, SocketAddr)> {
    let value = tokio::select! {
      conn = self.listener.accept() => Some(conn.unwrap()),
      _ = token.cancelled() => {
        info!("Standard listener shutdown!");
        None
      }
    };
    value
  }
}
