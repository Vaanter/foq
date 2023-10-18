use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use s2n_quic::Connection;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::data_channels::data_channel_wrapper::DataChannelWrapper;
use crate::handlers::connection_handler::AsyncReadWrite;

pub(crate) struct QuicOnlyDataChannelWrapper {
  addr: SocketAddr,
  data_channel: Arc<Mutex<Option<Box<dyn AsyncReadWrite>>>>,
  connection: Arc<Mutex<Connection>>,
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
  /// The port is set to 0.
  /// - `connection`: An [`Arc<Mutex<Connection>>`] containing the clients connection.
  ///
  /// # Returns
  ///
  /// A new instance of [`QuicOnlyDataChannelWrapper`].
  ///
  pub(crate) fn new(mut addr: SocketAddr, connection: Arc<Mutex<Connection>>) -> Self {
    addr.set_port(0);
    QuicOnlyDataChannelWrapper {
      addr,
      data_channel: Arc::new(Mutex::new(None)),
      connection,
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
  async fn create_stream(&mut self) -> Result<SocketAddr, Box<dyn Error>> {
    debug!("Creating passive listener");
    let conn = self.connection.clone();
    let mut data_channel = self.data_channel.clone().lock_owned().await;
    tokio::spawn(async move {
      debug!("Awaiting passive connection");
      let conn = tokio::time::timeout(Duration::from_secs(20), {
        conn.lock().await.accept_bidirectional_stream()
      })
      .await;

      match conn {
        Ok(Ok(Some(stream))) => {
          debug!(
            "Passive listener connection successful! ID: {}.",
            stream.id()
          );
          let _ = data_channel.insert(Box::new(stream));
        }
        Ok(Ok(None)) => warn!("Connection closed while awaiting stream!"),
        Ok(Err(e)) => warn!("Passive listener connection failed! {e}"),
        Err(e) => info!("Client failed to connect to passive listener before timeout! {e}"),
      };
    });

    Ok(self.addr)
  }
}

#[async_trait]
impl DataChannelWrapper for QuicOnlyDataChannelWrapper {
  /// Opens a data channel using [`QuicOnlyDataChannelWrapper::create_stream`].
  async fn open_data_stream(&mut self) -> Result<SocketAddr, Box<dyn Error>> {
    self.create_stream().await
  }

  async fn get_data_stream(&self) -> Arc<Mutex<Option<Box<dyn AsyncReadWrite>>>> {
    self.data_channel.clone()
  }

  async fn close_data_stream(&mut self) {
    let mut dc = self.data_channel.lock().await;
    if dc.is_some() {
      let _ = dc.as_mut().unwrap().shutdown().await;
    };
  }

  async fn get_addr(&self) -> &SocketAddr {
    &self.addr
  }
}
