use async_trait::async_trait;
use s2n_quic::stream::BidirectionalStream;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;

/// Handles clients connection.
#[async_trait]
pub(crate) trait ConnectionHandler {
  /// The entrypoint for handling a clients connection.
  /// 
  /// Starts by sending the client a server hello message. Then enters the command loop, where 
  /// it listens for incoming requests and evaluates them.
  async fn handle(&mut self, token: CancellationToken) -> Result<(), anyhow::Error>;
}

/// Types that implement this trait allow for asynchronous thread-safe reading and writing.
pub(crate) trait AsyncReadWrite: AsyncRead + AsyncWrite + Sync + Send + Unpin {}

/// Needed for reply sender
impl AsyncReadWrite for TcpStream {}
/// Needed for reply sender
impl AsyncReadWrite for BidirectionalStream {}
