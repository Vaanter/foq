use std::io::Error;
use std::sync::Arc;
use async_trait::async_trait;

use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter, WriteHalf};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::io::reply::Reply;

#[derive(Clone, Debug)]
pub(crate) struct ReplySender<T: AsyncWrite + Sync + Send> {
  writer: Arc<Mutex<BufWriter<WriteHalf<T>>>>,
}

impl<T: AsyncWrite + Sync + Send> ReplySender<T> {
  pub(crate) fn new(writer: WriteHalf<T>) -> Self {
    ReplySender {
      writer: Arc::new(Mutex::new(BufWriter::new(writer))),
    }
  }
}

#[async_trait]
impl <T: AsyncWrite + Sync + Send> ReplySend for ReplySender<T> {
  #[tracing::instrument(skip(self))]
  async fn send_control_message(&self, reply: Reply) {
    info!("Sending reply: {}", reply.to_string().trim());
    let mut writer = self.writer.lock().await;
    if let Err(e) = writer.write(reply.to_string().as_bytes()).await {
      error!("Failed to send reply! Error: {}", e);
    };
    if let Err(e) = writer.flush().await {
      warn!("Failed to flush reply! Error: {}", e);
    }
  }

  async fn close(&mut self) -> Result<(), Error> {
    self.writer.lock().await.shutdown().await
  }
}

#[async_trait]
pub(crate) trait ReplySend: Sync + Send {
  async fn send_control_message(&self, reply: Reply);
  async fn close(&mut self) -> Result<(), Error>;
}
