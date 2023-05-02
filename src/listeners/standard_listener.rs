use std::error::Error;
use std::net::SocketAddr;
use std::net::TcpListener as StdTcpListener;

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::Receiver;

use crate::handlers::standard_connection_handler::StandardConnectionHandler;
use tracing::info;

pub(crate) struct StandardListener {
    listener: TcpListener,
    handlers: Vec<StandardConnectionHandler>,
    shutdown_receiver: Receiver<()>,
}

impl StandardListener {
    pub(crate) fn new(
        addr: SocketAddr,
        shutdown_receiver: Receiver<()>,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(StandardListener {
            listener: TcpListener::from_std(StdTcpListener::bind(addr)?)?,
            handlers: vec![],
            shutdown_receiver,
        })
    }

    pub(crate) async fn accept(&mut self) -> Result<TcpStream, anyhow::Error> {
        let value = tokio::select! {
          stream = self.listener.accept() => Ok(stream.unwrap().0),
          _ = self.shutdown_receiver.recv() => Err(anyhow::anyhow!("Standard listener shutdown!"))
        };
        value
    }
  #[tracing::instrument(skip(self))]
        info!("Quic listener shutdown!");
}
