use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::handlers::connection_handler::AsyncReadWrite;

pub(crate) type DataChannel = Arc<Mutex<Option<Box<dyn AsyncReadWrite>>>>;

/// This trait specifies operations that can be used on a data channel.
#[async_trait]
pub(crate) trait DataChannelWrapper: Sync + Send {
  async fn open_data_stream(&mut self) -> Result<SocketAddr, Box<dyn Error>>;
  fn get_data_stream(&self) -> (DataChannel, CancellationToken);
  async fn close_data_stream(&mut self);
  fn get_addr(&self) -> &SocketAddr;
  fn abort(&self);
}
