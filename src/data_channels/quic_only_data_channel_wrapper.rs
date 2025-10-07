use anyhow::bail;
use async_channel::{Receiver, Sender, unbounded};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use s2n_quic::Connection;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, Span, debug, error, info, warn};

use crate::data_channels::data_channel_wrapper::{DataChannel, DataChannelWrapper};
use crate::data_channels::quic_data_channel::QuicDataChannel;
use crate::session::protection_mode::ProtMode;

pub(crate) struct QuicOnlyDataChannelWrapper {
  addr: SocketAddr,
  connection: Arc<Mutex<Connection>>,
  abort_token: CancellationToken,
  stream_sender: Sender<DataChannel>,
  stream_receiver: Receiver<DataChannel>,
}

impl QuicOnlyDataChannelWrapper {
  /// Creates a new instance of a [`QuicOnlyDataChannelWrapper`].
  ///
  /// This function takes in a [`SocketAddr`] and an [`Arc<Mutex<Connection>>`], and returns a new
  /// [`QuicOnlyDataChannelWrapper`] instance.
  /// The [`QuicOnlyDataChannelWrapper`] represents a wrapper for a QUIC-only data channel which
  /// uses streams for sending data.
  ///
  /// # Arguments
  ///
  /// - `addr`: A [`SocketAddr`] representing the address for the data channel.
  ///   The port is set to 0.
  /// - `connection`: An [`Arc<Mutex<Connection>>`] containing the clients' connection.
  ///
  /// # Returns
  ///
  /// A new instance of [`QuicOnlyDataChannelWrapper`].
  ///
  pub(crate) fn new(mut addr: SocketAddr, connection: Arc<Mutex<Connection>>) -> Self {
    addr.set_port(0);
    let (sender, receiver) = unbounded();
    QuicOnlyDataChannelWrapper {
      addr,
      connection,
      abort_token: CancellationToken::new(),
      stream_sender: sender,
      stream_receiver: receiver,
    }
  }

  /// Creates a new stream for the data channel.
  ///
  /// Creates a new [`tokio::task`] that waits 20 seconds for the client to create new
  /// bidirectional stream. If the client creates the stream, then the `data_channel` property is
  /// set and the data channel can be used. If the connection is closed, the client does not accept
  /// the stream in time or some other error occurs it will be logged.
  ///
  #[tracing::instrument(skip(self))]
  async fn create_stream(&self) -> Result<SocketAddr, anyhow::Error> {
    debug!("Creating passive listener");
    let conn = self.connection.clone();
    let sender = self.stream_sender.clone();
    let span = Span::current();
    tokio::spawn(
      async move {
        debug!("Awaiting passive connection");
        if let Ok(mut conn_lock) = timeout(Duration::from_secs(3), conn.lock()).await {
          let conn =
            timeout(Duration::from_secs(20), conn_lock.accept_bidirectional_stream()).await;
          match conn {
            Ok(Ok(Some(stream))) => {
              debug!("Passive listener connection successful! ID: {}.", stream.id());
              if let Err(mut e) = sender.send(Box::new(QuicDataChannel::new(stream))).await {
                error!("Failed to send new data channel downstream! {e}");
                if let Err(shutdown_error) = e.0.shutdown().await {
                  error!("Failed to shutdown data channel! {shutdown_error}");
                }
              }
            }
            Ok(Ok(None)) => warn!("Connection closed while awaiting stream!"),
            Ok(Err(e)) => warn!("Passive listener connection failed! {e}"),
            Err(e) => info!("Client failed to connect to passive listener before timeout! {e}"),
          };
        }
      }
      .instrument(span),
    );

    Ok(self.addr)
  }
}

#[async_trait]
impl DataChannelWrapper for QuicOnlyDataChannelWrapper {
  /// Opens a data channel using [`QuicOnlyDataChannelWrapper::create_stream`].
  async fn open_data_stream(&self, _prot_mode: ProtMode) -> Result<SocketAddr, anyhow::Error> {
    self.create_stream().await
  }

  fn try_acquire(&self) -> Result<(DataChannel, CancellationToken), anyhow::Error> {
    match self.stream_receiver.try_recv() {
      Ok(stream) => Ok((stream, self.abort_token.clone())),
      Err(e) => {
        bail!(e)
      }
    }
  }

  async fn acquire(&self) -> Result<(DataChannel, CancellationToken), anyhow::Error> {
    debug!("Acquiring data channel");
    match self.stream_receiver.recv().await {
      Ok(stream) => Ok((stream, self.abort_token.clone())),
      Err(e) => {
        bail!(e)
      }
    }
  }

  async fn close_data_stream(&self) {
    debug!("Closing all open data streams");
    while let Ok(mut stream) = self.stream_receiver.try_recv() {
      if let Err(e) = stream.shutdown().await {
        warn!("Failed to shutdown data channel {e}");
      };
    }
  }

  fn abort(&self) {
    self.abort_token.cancel();
  }
}
