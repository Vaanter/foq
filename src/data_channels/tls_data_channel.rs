use std::io::Error;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

pub(crate) struct TlsDataChannel(Pin<Box<TlsStream<TcpStream>>>);

impl TlsDataChannel {
  pub(crate) fn new(stream: TlsStream<TcpStream>) -> Self {
    TlsDataChannel(Box::pin(stream))
  }
}

impl AsyncRead for TlsDataChannel {
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<std::io::Result<()>> {
    self.0.as_mut().poll_read(cx, buf)
  }
}

impl AsyncWrite for TlsDataChannel {
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
    self.0.as_mut().poll_shutdown(cx)
  }
}
