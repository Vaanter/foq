extern crate core;

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::io::{Error, ErrorKind};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn, Level};

use crate::auth::auth_provider::AuthProvider;
use crate::auth::sqlite_data_source::SqliteDataSource;
use crate::global_context::{AUTH_PROVIDER, CERTS, DB_LAZY, KEY};
use crate::handlers::connection_handler::ConnectionHandler;
use crate::handlers::quic_only_connection_handler::QuicOnlyConnectionHandler;
use crate::handlers::standard_connection_handler::StandardConnectionHandler;
use crate::handlers::standard_tls_connection_handler::StandardTlsConnectionHandler;
use crate::listeners::quic_only_listener::QuicOnlyListener;
use crate::listeners::standard_listener::StandardListener;

mod auth;
mod commands;
mod global_context;
mod handlers;
mod io;
mod lab;
mod listeners;
mod utils;
mod data_channels;
mod session;

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
#[tracing::instrument]
async fn main() {
  let subscriber = tracing_subscriber::fmt()
    // Use a more compact, abbreviated log format
    .compact()
    // Display source code file paths
    .with_file(true)
    // Display source code line numbers
    .with_line_number(true)
    // Display the thread ID an event was recorded on
    .with_thread_ids(true)
    // Don't display the event's target (module path)
    .with_target(false)
    // Display logs from this level
    .with_max_level(Level::TRACE)
    // Build the subscriber
    .finish();
  tracing::subscriber::set_global_default(subscriber).unwrap();

  AUTH_PROVIDER
    .get_or_init(|| async {
      info!("Setting up auth provider.");
      let mut provider = AuthProvider::new();
      provider.add_data_source(Box::new(SqliteDataSource::new(DB_LAZY.clone())));
      provider
    })
    .await;

  let tcp_addr = "0.0.0.0:21".parse().unwrap();
  let tcp_tls_addr = "0.0.0.0:990".parse().unwrap();
  let quic_addr = "0.0.0.0:9900".parse().unwrap();

  let cancellation_token = CancellationToken::new();

  let tcp_task = tokio::spawn(run_tcp(tcp_addr, cancellation_token.clone()));
  let tcp_tls_task = tokio::spawn(run_tcp_tls(tcp_tls_addr, cancellation_token.clone()));
  let quic_task = tokio::spawn(run_quic(quic_addr, cancellation_token.clone()));

  match tokio::signal::ctrl_c().await {
    Ok(()) => {
      info!("Ctrl-c received!");
      cancellation_token.cancel();
      tcp_task.abort();
      tcp_tls_task.abort();
      quic_task.abort();
    }
    Err(e) => {
      error!("Ctrl-c signal error! {e}");
    }
  }
}

#[tracing::instrument]
async fn run_tcp(addr: SocketAddr, token: CancellationToken) -> Result<(), Error> {
  let mut standard_listener = StandardListener::new(addr).await.unwrap();
  debug!("[TCP] Running standard listener loop.");
  loop {
    let cancel = token.clone();
    match standard_listener.accept(cancel.clone()).await {
      Some((stream, addr)) => {
        info!("[TCP] Received connection from: {:?}", addr);
        debug!("[TCP] Creating handler for connection from {:?}", addr);
        tokio::spawn(async move {
          let mut handler = StandardConnectionHandler::new(stream);
          if let Err(e) = handler.handle(cancel).await {
            error!("{:?}", e);
          };
        });
      }
      None => {
        break;
      }
    };
  }
  Ok(())
}

#[tracing::instrument]
async fn run_tcp_tls(addr: SocketAddr, token: CancellationToken) -> Result<(), Error> {
  let config = rustls::ServerConfig::builder()
    .with_safe_defaults()
    .with_no_client_auth()
    .with_single_cert(CERTS.clone(), KEY.clone())
    .map_err(|err| Error::new(ErrorKind::InvalidInput, err))?;
  let tls_acceptor = TlsAcceptor::from(Arc::new(config));
  let mut standard_listener = StandardListener::new(addr).await.unwrap();
  debug!("[TCP+TLS] Running standard listener loop.");
  loop {
    let cancel = token.clone();
    match standard_listener.accept(cancel.clone()).await {
      Some((stream, addr)) => {
        info!("[TCP+TLS] Received connection from: {:?}", addr);
        let acceptor = tls_acceptor.clone();
        tokio::spawn(async move {
          debug!("[TCP+TLS] Creating handler for connection from {:?}", addr);
          let tls_stream: TlsStream<TcpStream> = match acceptor.accept(stream).await {
            Ok(t) => t,
            Err(e) => {
              info!("Unable to create TLS connection. Error: {e}");
              return;
            }
          };
          let mut handler = StandardTlsConnectionHandler::new(tls_stream);
          if let Err(e) = handler.handle(cancel).await {
            error!("{:?}", e);
          };
        });
      }
      None => {
        break;
      }
    };
  }
  Ok(())
}

#[tracing::instrument]
async fn run_quic(addr: SocketAddr, token: CancellationToken) {
  let mut standard_listener = QuicOnlyListener::new(addr).unwrap();
  debug!("[QUIC] Running quic listener loop.");
  loop {
    let cancel = token.clone();
    match standard_listener.accept(cancel.clone()).await {
      Some(conn) => {
        let peer = conn.remote_addr().unwrap();
        info!("[QUIC] Received connection from: {:?}", peer);
        tokio::spawn(async move {
          debug!("[QUIC] Creating handler for connection from {:?}", peer);
          let mut handler = QuicOnlyConnectionHandler::new(conn);
          if let Err(e) = handler.handle(cancel).await {
            error!("{:?}", e);
          };
        });
      }
      None => {
        break;
      }
    }
  }
}
