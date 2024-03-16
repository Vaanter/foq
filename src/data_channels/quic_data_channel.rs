use std::io::Error;
use std::pin::Pin;
use std::task::{Context, Poll};

use s2n_quic::stream::BidirectionalStream;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tracing::debug;

pub(crate) struct QuicDataChannel(Pin<Box<BidirectionalStream>>);

impl QuicDataChannel {
  pub(crate) fn new(stream: BidirectionalStream) -> Self {
    QuicDataChannel(Box::pin(stream))
  }
}

impl AsyncRead for QuicDataChannel {
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<std::io::Result<()>> {
    self.0.as_mut().poll_read(cx, buf)
  }
}

impl AsyncWrite for QuicDataChannel {
  fn poll_write(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &[u8],
  ) -> Poll<Result<usize, Error>> {
    self.0.as_mut().poll_write(cx, buf)
  }

  fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
    self.0.as_mut().poll_flush(cx).map_err(Error::from)
  }

  fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
    let poll_state = self.0.as_mut().poll_shutdown(cx);
    if poll_state.is_ready() {
      debug!("Shutting down stream {}", self.0.id());
    }
    poll_state
  }
}
