use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{debug, info, warn};

use crate::handlers::connection_handler::AsyncReadWrite;
use crate::data_channels::data_channel_wrapper::DataChannelWrapper;

pub(crate) struct StandardDataChannelWrapper {
  addr: SocketAddr,
  data_channel: Arc<Mutex<Option<Box<dyn AsyncReadWrite>>>>,
}

impl StandardDataChannelWrapper {
  /// Creates a new instance of a [`StandardDataChannelWrapper`].
  ///
  /// This function takes in a [`SocketAddr`] and returns a new [`StandardDataChannelWrapper`]
  /// instance.
  /// The [`StandardDataChannelWrapper`] represents a wrapper for a TCP (unencrypted) data channel
  /// that is used for sending data.
  ///
  /// # Arguments
  ///
  /// - `addr`: A [`SocketAddr`] representing the address for the data channel.
  /// The port is set to 0.
  ///
  /// # Returns
  ///
  /// A new instance of [`StandardDataChannelWrapper`].
  ///
  pub(crate) fn new(mut addr: SocketAddr) -> Self {
    addr.set_port(0);
    StandardDataChannelWrapper {
      addr,
      data_channel: Arc::new(Mutex::new(None)),
    }
  }

  /// Creates a new stream for the data channel.
  ///
  /// Creates a new [`tokio::task`] that creates a new TCP listener which waits for up to 20
  /// seconds for the client to connect. If the client connects, then the `data_channel` property
  /// is set and the data channel can be used. If the client does not connect in time or some
  /// other error occurs it will logged.
  ///
  /// # Panics
  ///
  /// This function will panic if all ports are in use.
  ///
  /// # Returns
  ///
  /// A [`SocketAddr`] the server listens on.
  ///
  #[tracing::instrument(skip(self))]
  async fn create_stream(&mut self) -> Result<SocketAddr, Box<dyn Error>> {
    debug!("Creating passive listener");
    let listener = TcpListener::bind(self.addr)
      .await
      .expect("Implement passive port search!");
    let port = listener.local_addr()?.port();
    let mut data_lock = self.data_channel.clone().lock_owned().await;
    tokio::spawn(async move {
      let conn = timeout(Duration::from_secs(20), {
        debug!("Awaiting passive connection");
        listener.accept()
      })
      .await;
      match conn {
        Ok(Ok((stream, _))) => {
          info!(
            "Passive connection created! Remote address: {}",
            stream
              .peer_addr()
              .expect("Passive data connection should have peer!")
          );
          let _ = data_lock.insert(Box::new(stream));
        }
        Ok(Err(e)) => {
          warn!("Passive listener connection failed! {e}");
        }
        Err(e) => {
          info!("Client failed to connect to passive listener before timeout! {e}");
        }
      };
    });

    let mut addr = self.addr.clone();
    addr.set_port(port);
    Ok(addr)
  }
}

#[async_trait]
impl DataChannelWrapper for StandardDataChannelWrapper {
  /// Opens a data channel using [`StandardDataChannelWrapper::create_stream`].
  async fn open_data_stream(&mut self) -> Result<SocketAddr, Box<dyn Error>> {
    self.close_data_stream().await;
    self.create_stream().await
  }

  async fn get_data_stream(&self) -> Arc<Mutex<Option<Box<dyn AsyncReadWrite>>>> {
    self.data_channel.clone()
  }

  async fn close_data_stream(&mut self) {
    let dc = self.data_channel.clone();
    if dc.lock().await.is_some() {
      let _ = dc.lock().await.as_mut().unwrap().shutdown().await;
    };
  }

  async fn get_addr(&self) -> &SocketAddr {
    &self.addr
  }
}
