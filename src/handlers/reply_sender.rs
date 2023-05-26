use std::io::Error;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter, WriteHalf};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::commands::reply::Reply;

/// A generic abstraction for sending replies to client.
#[derive(Clone, Debug)]
pub(crate) struct ReplySender<T: AsyncWrite + Sync + Send> {
  writer: Arc<Mutex<BufWriter<WriteHalf<T>>>>,
}

impl<T: AsyncWrite + Sync + Send> ReplySender<T> {
  /// Constructs a new reply sender.
  ///
  /// Creates a new [`BufWriter`] from the argument, that can be later used to send messages to
  /// client.
  ///
  pub(crate) fn new(writer: WriteHalf<T>) -> Self {
    ReplySender {
      writer: Arc::new(Mutex::new(BufWriter::new(writer))),
    }
  }
}

#[async_trait]
impl<T: AsyncWrite + Sync + Send> ReplySend for ReplySender<T> {
  #[tracing::instrument(skip(self))]
  /// Sends a reply to the client.
  ///
  /// Writes and flushes a message to control channel. If writing or flushing fails it will be
  /// reported although no error is returned.
  ///
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

  /// Closes the writer which closes the whole control channel.
  async fn close(&mut self) -> Result<(), Error> {
    debug!("Closing sender half.");
    self.writer.lock().await.shutdown().await
  }
}

/// Specifies functions required to send a message to client.
///
/// Implementors must be thread-safe.
#[async_trait]
pub(crate) trait ReplySend: Sync + Send {
  async fn send_control_message(&self, reply: Reply);
  async fn close(&mut self) -> Result<(), Error>;
}
