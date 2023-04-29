use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::handlers::connection_handler::AsyncReadWrite;

#[async_trait]
pub(crate) trait DataChannelWrapper: Sync + Send {
    async fn open_data_stream(&mut self) -> Result<SocketAddr, Box<dyn Error>>;
    async fn get_data_stream(&self) -> Arc<Mutex<Option<Box<dyn AsyncReadWrite>>>>;
    async fn close_data_stream(&mut self);
    async fn get_addr(&self) -> &SocketAddr;
}
