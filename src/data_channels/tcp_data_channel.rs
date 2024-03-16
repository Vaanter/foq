use std::io::Error;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tracing::debug;

pub(crate) struct TcpDataChannel(Pin<Box<TcpStream>>);

impl TcpDataChannel {
  pub(crate) fn new(stream: TcpStream) -> Self {
    TcpDataChannel(Box::pin(stream))
  }
}

impl AsyncRead for TcpDataChannel {
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<std::io::Result<()>> {
    self.0.as_mut().poll_read(cx, buf)
  }
}

impl AsyncWrite for TcpDataChannel {
  fn poll_write(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &[u8],
  ) -> Poll<Result<usize, Error>> {
    self.0.as_mut().poll_write(cx, buf)
  }

  fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
    self.0.as_mut().poll_flush(cx)
  }

  fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
    let poll_state = self.0.as_mut().poll_shutdown(cx);
    if poll_state.is_ready() {
      debug!(
        "Shutting down connection from {}",
        self
          .0
          .peer_addr()
          .map(|x| x.to_string())
          .unwrap_or(String::from("UNKNOWN"))
      );
    }
    poll_state
  }
}
