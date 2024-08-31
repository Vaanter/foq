use quinn::{RecvStream, SendStream};
use std::io::Error;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tracing::debug;

pub(crate) struct QuinnDataChannel {
  send_stream: Pin<Box<SendStream>>,
  recv_stream: Pin<Box<RecvStream>>,
}

impl QuinnDataChannel {
  pub(crate) fn new(send_stream: SendStream, recv_stream: RecvStream) -> Self {
    QuinnDataChannel {
      send_stream: Box::pin(send_stream),
      recv_stream: Box::pin(recv_stream),
    }
  }
}

impl AsyncRead for QuinnDataChannel {
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<std::io::Result<()>> {
    AsyncRead::poll_read(self.recv_stream.as_mut(), cx, buf)
  }
}

impl AsyncWrite for QuinnDataChannel {
  fn poll_write(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &[u8],
  ) -> Poll<Result<usize, Error>> {
    AsyncWrite::poll_write(self.send_stream.as_mut(), cx, buf)
  }

  fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
    AsyncWrite::poll_flush(self.send_stream.as_mut(), cx)
  }

  fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
    let poll_state = AsyncWrite::poll_shutdown(self.send_stream.as_mut(), cx);
    if poll_state.is_ready() {
      debug!("Shutting down stream {}", self.recv_stream.id());
    }
    poll_state
  }
}
