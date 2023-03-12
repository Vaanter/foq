use std::sync::Arc;
use async_trait::async_trait;

use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter, WriteHalf};
use tokio::sync::Mutex;

use crate::io::reply::Reply;

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
  async fn send_control_message(&self, reply: Reply) {
    println!("Reply to send: {}", reply.to_string().trim());
    let mut writer = self.writer.lock().await;
    if let Err(e) = writer.write(reply.to_string().as_bytes()).await {
      eprintln!("Error sending reply! {}", e);
    };
    if let Err(e) = writer.flush().await {
      eprintln!("Failed to flush after reply: {}", e);
    }
    println!("Reply sent");
  }
}

#[async_trait]
pub(crate) trait ReplySend: Sync + Send {
  async fn send_control_message(&self, reply: Reply);
}
