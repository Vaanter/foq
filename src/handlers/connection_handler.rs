use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};
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

/// Blanket implementation needed for data channel
impl<T> AsyncReadWrite for T where T: AsyncRead + AsyncWrite + Sync + Send + Unpin {}
