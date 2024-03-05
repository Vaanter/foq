use anyhow::bail;
use async_channel::{unbounded, Receiver, Sender};
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace, warn};

use crate::data_channels::data_channel_wrapper::{DataChannel, DataChannelWrapper};

pub(crate) struct StandardDataChannelWrapper {
  addr: SocketAddr,
  channel_sender: Sender<DataChannel>,
  channel_receiver: Receiver<DataChannel>,
  abort_token: CancellationToken,
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
    let (sender, receiver) = unbounded();
    StandardDataChannelWrapper {
      addr,
      channel_sender: sender,
      channel_receiver: receiver,
      abort_token: CancellationToken::new(),
    }
  }

  /// Creates a new stream for the data channel.
  ///
  /// Creates a new [`tokio::task`] that creates a new TCP listener which waits for up to 20
  /// seconds for the client to connect. If the client connects, then the `data_channel` property
  /// is set and the data channel can be used. If the client does not connect in time or some
  /// other error occurs it will be logged.
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
  async fn create_stream(&self) -> Result<SocketAddr, Box<dyn Error>> {
    debug!("Creating passive listener");
    let listener = TcpListener::bind(self.addr)
      .await
      .expect("Implement passive port search!");
    let port = listener.local_addr()?.port();
    let sender = self.channel_sender.clone();
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
          if let Err(mut e) = sender.send(Box::new(stream)).await {
            error!("Failed to send new data channel downstream! {e}");
            if let Err(shutdown_error) = e.0.shutdown().await {
              error!("Failed to shutdown data channel! {shutdown_error}");
            }
          }
        }
        Ok(Err(e)) => {
          warn!("Passive listener connection failed! {e}");
        }
        Err(e) => {
          info!("Client failed to connect to passive listener before timeout! {e}");
        }
      };
    });

    let mut addr = self.addr;
    addr.set_port(port);
    Ok(addr)
  }
}

#[async_trait]
impl DataChannelWrapper for StandardDataChannelWrapper {
  /// Opens a data channel using [`StandardDataChannelWrapper::create_stream`].
  async fn open_data_stream(&self) -> Result<SocketAddr, Box<dyn Error>> {
    self.create_stream().await
  }

  fn try_acquire(&self) -> Result<(DataChannel, CancellationToken), anyhow::Error> {
    match self.channel_receiver.try_recv() {
      Ok(stream) => Ok((stream, self.abort_token.clone())),
      Err(e) => {
        bail!(e)
      }
    }
  }

  async fn acquire(&self) -> Result<(DataChannel, CancellationToken), anyhow::Error> {
    return match self.channel_receiver.recv().await {
      Ok(stream) => Ok((stream, self.abort_token.clone())),
      Err(e) => {
        bail!(e)
      }
    };
  }

  #[tracing_attributes::instrument(skip(self))]
  async fn close_data_stream(&self) {
    trace!("Shutting down data channel");
    while let Ok(mut stream) = self.channel_receiver.try_recv() {
      if let Err(e) = stream.shutdown().await {
        warn!("Failed to shutdown data channel {e}");
      };
    }
  }

  fn get_addr(&self) -> &SocketAddr {
    &self.addr
  }

  fn abort(&self) {
    self.abort_token.cancel();
  }
}
