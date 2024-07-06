use anyhow::bail;
use async_channel::{unbounded, Receiver, Sender};
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace, warn};

use crate::data_channels::data_channel_wrapper::{DataChannel, DataChannelWrapper};
use crate::data_channels::tcp_data_channel::TcpDataChannel;
use crate::data_channels::tls_data_channel::TlsDataChannel;
use crate::global_context::TLS_CONFIG;
use crate::session::protection_mode::ProtMode;

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
  async fn create_stream(&self, prot_mode: ProtMode) -> Result<SocketAddr, Box<dyn Error>> {
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
          Self::establish_connection(stream, prot_mode, sender).await;
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

  async fn establish_connection(
    mut stream: TcpStream,
    prot_mode: ProtMode,
    sender: Sender<DataChannel>,
  ) {
    let peer = stream
      .peer_addr()
      .expect("Data channel should have peer address");
    info!("Passive connection created! Remote address: {:?}", peer);
    let tls = TLS_CONFIG.clone().map(TlsAcceptor::from);
    let data_channel = match prot_mode {
      ProtMode::Private => {
        trace!(peer_addr = ?peer, "Opening data channel in protected mode.");
        match tls {
          Some(t) => {
            trace!(peer_addr = ?peer, "TLS handshake.");
            match timeout(Duration::from_secs(10), t.accept(stream)).await {
              Ok(Ok(tls_stream)) => {
                let tls_stream = Box::new(TlsDataChannel::new(tls_stream)) as DataChannel;
                trace!(peer_addr = ?peer, "TLS handshake complete.");
                tls_stream
              }
              Ok(Err(e)) => {
                info!(peer_addr = ?peer, "TLS handshake failed, closing connection! {e}.");
                return;
              }
              Err(e) => {
                info!(peer_addr = ?peer, "TLS handshake failed to complete in time, closing connection! {e}.");
                return;
              }
            }
          }
          None => {
            debug!("TLS not available, closing connection");
            if let Err(e) = stream.shutdown().await {
              warn!(peer_addr = ?peer, "Failed to close invalid data channel! {e}.");
            };
            return;
          }
        }
      }
      ProtMode::Clear | ProtMode::Confidential | ProtMode::Safe => {
        Box::new(TcpDataChannel::new(stream)) as DataChannel
      }
    };
    if let Err(mut e) = sender.send(data_channel).await {
      error!("Failed to send new data channel downstream! {e}");
      if let Err(shutdown_error) = e.0.shutdown().await {
        error!("Failed to shutdown data channel! {shutdown_error}");
      }
    }
  }
}

#[async_trait]
impl DataChannelWrapper for StandardDataChannelWrapper {
  /// Opens a data channel using [`StandardDataChannelWrapper::create_stream`].
  async fn open_data_stream(&self, prot_mode: ProtMode) -> Result<SocketAddr, Box<dyn Error>> {
    self.create_stream(prot_mode).await
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
