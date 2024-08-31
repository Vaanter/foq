use anyhow::bail;
use async_channel::{unbounded, Receiver, Sender};
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use quinn::Connection;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn, Instrument, Span};

use crate::data_channels::data_channel_wrapper::{DataChannel, DataChannelWrapper};
use crate::data_channels::quinn_data_channel::QuinnDataChannel;
use crate::session::protection_mode::ProtMode;

pub(crate) struct QuicQuinnDataChannelWrapper {
  addr: SocketAddr,
  connection: Arc<Mutex<Connection>>,
  abort_token: CancellationToken,
  stream_sender: Sender<DataChannel>,
  stream_receiver: Receiver<DataChannel>,
}

impl QuicQuinnDataChannelWrapper {
  /// Creates a new instance of a [`QuicQuinnDataChannelWrapper`].
  ///
  /// This function takes in a [`SocketAddr`] and an [`Arc<Mutex<Connection>>`], and returns a new
  /// [`QuicQuinnDataChannelWrapper`] instance.
  /// The [`QuicQuinnDataChannelWrapper`] represents a wrapper for a QUIC-based data channel which
  /// uses streams for sending data.
  ///
  /// # Arguments
  ///
  /// - `addr`: A [`SocketAddr`] representing the address for the data channel.
  /// - `connection`: An [`Arc<Mutex<Connection>>`] containing the clients' connection.
  ///
  /// # Returns
  ///
  /// A new instance of [`QuicQuinnDataChannelWrapper`].
  ///
  pub(crate) fn new(addr: SocketAddr, connection: Arc<Mutex<Connection>>) -> Self {
    let (sender, receiver) = unbounded();
    QuicQuinnDataChannelWrapper {
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
  async fn create_stream(&self) -> Result<SocketAddr, Box<dyn Error>> {
    debug!("Creating passive listener");
    let conn = self.connection.clone();
    let sender = self.stream_sender.clone();
    let span = Span::current();
    tokio::spawn(
      async move {
        debug!("Awaiting passive connection");
        let conn = timeout(Duration::from_secs(20), conn.lock().await.accept_bi()).await;

        match conn {
          Ok(Ok((send_stream, recv_stream))) => {
            debug!(
              "Passive listener connection successful! ID: {}.",
              send_stream.id()
            );
            if let Err(mut e) = sender
              .send(Box::new(QuinnDataChannel::new(send_stream, recv_stream)))
              .await
            {
              error!("Failed to send new data channel downstream! {e}");
              if let Err(shutdown_error) = e.0.shutdown().await {
                error!("Failed to shutdown data channel! {shutdown_error}");
              }
            }
          }
          Ok(Err(e)) => warn!("Connection closed while awaiting stream! {e}"),
          Err(e) => info!("Client failed to connect to passive listener before timeout! {e}"),
        };
      }
      .instrument(span),
    );

    Ok(self.addr)
  }
}

#[async_trait]
impl DataChannelWrapper for QuicQuinnDataChannelWrapper {
  /// Opens a data channel using [`QuicQuinnDataChannelWrapper::create_stream`].
  async fn open_data_stream(&self, _prot_mode: ProtMode) -> Result<SocketAddr, Box<dyn Error>> {
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
