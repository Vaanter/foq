//! Execution point for all listeners.

use crate::auth::auth_provider::AuthProvider;
use crate::auth::sqlite_data_source::SqliteDataSource;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::server::TlsStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::global_context::{AUTH_PROVIDER, CONFIG, DB_LAZY, TLS_CONFIG};
use crate::handlers::connection_handler::ConnectionHandler;
use crate::handlers::quic_only_connection_handler::QuicOnlyConnectionHandler;
use crate::handlers::quic_quinn_connection_handler::QuicQuinnConnectionHandler;
use crate::handlers::standard_connection_handler::StandardConnectionHandler;
use crate::handlers::standard_tls_connection_handler::StandardTlsConnectionHandler;
use crate::listeners::quic_only_listener::QuicOnlyListener;
use crate::listeners::quinn_listener::QuinnListener;
use crate::listeners::standard_listener::StandardListener;

/// Starts all available listeners.
///
/// # Auth setup
/// The authentication backend ([`AUTH_PROVIDER`]) is initialized with [`SqliteDataSource`]
/// as the only source. If the SQLite connection is invalid, this will panic.
///
/// # Listener setup
/// The TCP, TCP+TLS and QUIC listeners are setup. If the IP address of a listener is not set in
/// config, then that listener is skipped. Each listener runs in it's own [`tokio::task`].
///
/// After the listeners are setup, the runner awaits for SIGINT which trigger a graceful shutdown.
///
///
pub(crate) async fn run() {
  AUTH_PROVIDER
    .get_or_init(|| async {
      debug!("Setting up auth provider.");
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

  let quinn_address = CONFIG.get_string("quinn_address");
  if let Ok(quinn_address) = quinn_address {
    match quinn_address.parse() {
      Ok(quinn_addr) => {
        let quinn_task = tokio::spawn(run_quinn(quinn_addr, cancellation_token.clone()));
        tasks.push(quinn_task);
      }
      Err(_) => error!("Failed to parse QUINN address!"),
    }
  }

  match tokio::signal::ctrl_c().await {
    Ok(()) => {
      info!("Ctrl-c received!");
      cancellation_token.cancel();
      for handle in tasks.iter_mut() {
        handle.await.expect("Handle exit error!");
      }
    }
    Err(e) => error!("Ctrl-c signal error! {e}"),
  }
}

/// Executes a TCP listener.
///
/// # Listener loop
/// First a [`listener`] is set up. If it fails, then this will report an error and exit.
/// After setup, the listener loop starts. Clients connections are passed into [`handler`],
/// which runs in a new [`tokio::task`].
///
/// [`listener`]: StandardListener
/// [`handler`]: StandardConnectionHandler
///
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

/// Executes a TCP+TLS listener.
///
/// # Config
/// TLS is backed by [`rustls`]. If the certificate or key is not set in the config, then this will
/// report an error and exit. Logging of session secrets will be enabled if the 'SSLKEYLOGFILE'
/// environment variable is set.
///
/// # Listener loop
/// After the setup, the listener loop starts. When the client creates a connection, TLS is applied
/// on the TCP connection. The connection is then passed into [`handler`] which runs in a new
/// [`tokio::task`]. This loop continues until the listener shuts down.
///
/// [`handler`]: StandardTlsConnectionHandler
///
#[tracing::instrument(skip(token))]
async fn run_tcp_tls(addr: SocketAddr, token: CancellationToken) {
  let tls_acceptor = match TLS_CONFIG.clone() {
    Some(tls_config) => TlsAcceptor::from(tls_config),
    None => {
      warn!("TLS not available, unable to start TLS listener!");
      return;
    }
  };
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
              info!(peer_name = ?addr, "Unable to create TLS connection. Error: {e}");
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

/// Executes a QUIC listener.
///
/// # Listener loop
/// First a [`listener`] is setup. If it fails, then this will report an error and exit.
/// After setup, the listener loop starts. Clients connections are passed into [`handler`],
/// which runs in a new [`tokio::task`].
///
/// [`listener`]: QuicOnlyListener
/// [`handler`]: QuicOnlyConnectionHandler
///
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
      Some(mut conn) => {
        let peer = conn.remote_addr().unwrap();
        conn.keep_alive(false).unwrap();
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

#[tracing::instrument(skip(token))]
async fn run_quinn(addr: SocketAddr, token: CancellationToken) {
  let quic_quinn_listener = match QuinnListener::new(addr) {
    Ok(l) => l,
    Err(e) => {
      error!("[QUINN] Failed to create listener! Error: {}", e);
      return;
    }
  };
  info!("[QUINN] Listening on {}.", addr);
  loop {
    let cancel = token.clone();
    match quic_quinn_listener.accept(cancel.clone()).await {
      Some(connection) => {
        let conn = match connection.await {
          Ok(c) => c,
          Err(e) => {
            error!("Failed to create connection! {e}");
            continue;
          }
        };
        let peer = conn.remote_address();
        info!("[QUINN] Received connection from: {:?}", peer);
        tokio::spawn(async move {
          debug!("[QUINN] Creating handler for connection from {:?}", peer);
          let mut handler = QuicQuinnConnectionHandler::new(conn);
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
