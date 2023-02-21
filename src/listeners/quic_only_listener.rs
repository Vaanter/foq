use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use s2n_quic::{provider::io::tokio::Builder as IoBuilder, Server};
use tokio::sync::Mutex;

use crate::handlers::quic_only_connection_handler::QuicOnlyConnectionHandler;
use crate::listeners::listenable::Listenable;

/// NOTE: this certificate is to be used for demonstration purposes only!
pub static CERT_PEM: &str = include_str!(concat!(
env!("CARGO_MANIFEST_DIR"),
"/certs/server-cert.pem"
));
/// NOTE: this certificate is to be used for demonstration purposes only!
pub static KEY_PEM: &str =
  include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/certs/server-key.pem"));

pub(crate) struct QuicOnlyListener {
  server: Server,
  handlers: Vec<QuicOnlyConnectionHandler>,
}

impl QuicOnlyListener {}

impl Listenable for QuicOnlyListener {
  fn listen(addr: SocketAddr) -> Result<Arc<Mutex<dyn Listenable>>, Box<dyn Error>> {
    let io = IoBuilder::default()
      .with_receive_address(addr)?
      .build()?;

    let mut server = Server::builder()
      .with_tls((CERT_PEM, KEY_PEM))?
      .with_io(io)?
      .start()?;

    let listener = Arc::from(Mutex::from(QuicOnlyListener {
      server,
      handlers: vec![],
    }));

    {
      let listener  = listener.clone();
      tokio::spawn(async move {
        println!("Listening on QUIC!");
        listener.lock().await.server.accept().await.unwrap();
        println!("No longer listening on QUIC!");
      });
    }

    Ok(listener)
  }

  fn stop_listening(&self) -> Result<(), Box<dyn Error>> {
    todo!()
  }
}