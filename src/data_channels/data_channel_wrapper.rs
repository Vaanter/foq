use std::net::SocketAddr;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::handlers::connection_handler::AsyncReadWrite;
use crate::session::protection_mode::ProtMode;

pub(crate) type DataChannel = Box<dyn AsyncReadWrite>;

/// This trait specifies operations that can be used on a data channel.
#[async_trait]
pub(crate) trait DataChannelWrapper: Sync + Send {
  async fn open_data_stream(&self, prot_mode: ProtMode) -> Result<SocketAddr, anyhow::Error>;
  fn try_acquire(&self) -> Result<(DataChannel, CancellationToken), anyhow::Error>;
  async fn acquire(&self) -> Result<(DataChannel, CancellationToken), anyhow::Error>;
  async fn close_data_stream(&self);
  fn abort(&self);
}
