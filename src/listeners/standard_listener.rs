use std::error::Error;
use std::net::SocketAddr;
use std::net::TcpListener as StdTcpListener;

use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;
use tracing::info;

pub(crate) struct StandardListener {
  listener: TcpListener,
}

impl StandardListener {
  pub(crate) async fn new(
    addr: SocketAddr,
  ) -> Result<Self, Box<dyn Error>> {
    Ok(StandardListener {
      listener: TcpListener::bind(addr).await?,
    })
  }

  #[tracing::instrument(skip(self))]
  pub(crate) async fn accept(&mut self, token: CancellationToken) -> Option<(TcpStream, SocketAddr)> {
    let value = tokio::select! {
      conn = self.listener.accept() => Some(conn.unwrap()),
      _ = token.cancelled() => {
        info!("Quic listener shutdown!");
        None
      }
    };
    value
  }
}
