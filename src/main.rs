extern crate core;

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use tokio::io::{Error, ErrorKind};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn, Level};

use crate::auth::auth_provider::AuthProvider;
use crate::auth::sqlite_data_source::SqliteDataSource;
use crate::global_context::{AUTH_PROVIDER, CERTS, CONFIG, DB_LAZY, KEY};
use crate::handlers::connection_handler::ConnectionHandler;
use crate::handlers::quic_only_connection_handler::QuicOnlyConnectionHandler;
use crate::handlers::standard_connection_handler::StandardConnectionHandler;
use crate::handlers::standard_tls_connection_handler::StandardTlsConnectionHandler;
use crate::listeners::quic_only_listener::QuicOnlyListener;
use crate::listeners::standard_listener::StandardListener;

mod auth;
mod commands;
mod data_channels;
mod global_context;
mod handlers;
mod io;
mod listeners;
mod session;
mod utils;

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
#[tracing::instrument]
async fn main() {
  let log_level = Level::from_str(&CONFIG.get_string("log_level").unwrap_or(String::new()))
    .unwrap_or(Level::INFO);
  let subscriber = tracing_subscriber::fmt()
    .with_file(true)
    .with_line_number(true)
    .with_thread_ids(true)
    .with_target(false)
    .with_max_level(log_level)
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

  let cancellation_token = CancellationToken::new();

  let mut tasks = Vec::with_capacity(3);
  let tcp_address = CONFIG.get_string("tcp_address");
  if let Ok(tcp_address) = tcp_address {
    match tcp_address.parse() {
      Ok(tcp_addr) => {
        let tcp_task = tokio::spawn(run_tcp(tcp_addr, cancellation_token.clone()));
        tasks.push(tcp_task);
      }
      Err(_) => error!("Failed to parse TCP address!"),
    }
  } else {
    warn!("No TCP address in config!");
  }

  let tcp_tls_address = CONFIG.get_string("tcp_tls_address");
  if let Ok(tcp_tls_address) = tcp_tls_address {
    match tcp_tls_address.parse() {
      Ok(tcp_tls_addr) => {
        let tcp_tls_task = tokio::spawn(run_tcp_tls(tcp_tls_addr, cancellation_token.clone()));
        tasks.push(tcp_tls_task);
      }
      Err(_) => error!("Failed to parse TCP+TLS address!"),
    }
  } else {
    warn!("No TCP+TLS address in config!");
  }

  let quic_address = CONFIG.get_string("quic_address");
  if let Ok(quic_address) = quic_address {
    match quic_address.parse() {
      Ok(quic_addr) => {
        let quic_task = tokio::spawn(run_quic(quic_addr, cancellation_token.clone()));
        tasks.push(quic_task);
      }
      Err(_) => error!("Failed to parse QUIC address!"),
    }
  } else {
    warn!("No QUIC address in config!");
  }

  match tokio::signal::ctrl_c().await {
    Ok(()) => {
      info!("Ctrl-c received!");
      cancellation_token.cancel();
      for handle in tasks.iter_mut() {
        handle.await.expect("TODO: panic message");
      }
    }
    Err(e) => error!("Ctrl-c signal error! {e}"),
  }
}

#[tracing::instrument(skip(token))]
async fn run_tcp(addr: SocketAddr, token: CancellationToken) {
  let mut standard_listener = match StandardListener::new(addr).await {
    Ok(l) => l,
    Err(e) => {
      error!("[TCP] Failed to create listener! Error: {}", e);
      return;
    }
  };
  info!("[TCP] Listening on {}.", addr);
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
}

#[tracing::instrument(skip(token))]
async fn run_tcp_tls(addr: SocketAddr, token: CancellationToken) {
  let mut config = match rustls::ServerConfig::builder()
    .with_safe_defaults()
    .with_no_client_auth()
    .with_single_cert(CERTS.clone(), KEY.clone())
    .map_err(|err| Error::new(ErrorKind::InvalidInput, err))
  {
    Ok(c) => c,
    Err(e) => {
      error!("[TCP+TLS] Error creating config! {e}");
      return;
    }
  };
  config.alpn_protocols = vec!["ftp".as_bytes().to_vec()];

  if std::env::var_os("SSLKEYLOGFILE").is_some() {
    config.key_log = Arc::new(rustls::KeyLogFile::new());
  }

  let tls_acceptor = TlsAcceptor::from(Arc::new(config));
  let mut standard_listener = match StandardListener::new(addr).await {
    Ok(l) => l,
    Err(e) => {
      error!("[TCP+TLS] Failed to create listener! Error: {}", e);
      return;
    }
  };

  info!("[TCP+TLS] Listening on {}.", addr);
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
}

#[tracing::instrument(skip(token))]
async fn run_quic(addr: SocketAddr, token: CancellationToken) {
  let mut quic_only_listener = match QuicOnlyListener::new(addr) {
    Ok(l) => l,
    Err(e) => {
      error!("[QUIC] Failed to create listener! Error: {}", e);
      return;
    }
  };
  info!("[QUIC] Listening on {}.", addr);
  loop {
    let cancel = token.clone();
    match quic_only_listener.accept(cancel.clone()).await {
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
