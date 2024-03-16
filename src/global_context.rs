//! Contains global statics

use std::io::{Error, ErrorKind};
use std::path::Path;
use std::sync::Arc;

use config::Config;
use once_cell::sync::Lazy;
use rustls::server::ServerSessionMemoryCache;
use rustls::{Certificate, PrivateKey, Ticketer};
use sqlx::SqlitePool;
use tokio::sync::OnceCell;
use tokio_rustls::TlsAcceptor;
use tracing::{info, warn};

use crate::auth::auth_provider::AuthProvider;
use crate::utils::tls_utils::{load_certs, load_keys};

/// The configuration loaded from config file
pub(crate) static CONFIG: Lazy<Config> = Lazy::new(|| {
  Config::builder()
    .add_source(config::File::with_name("config.toml"))
    // Add in settings from the environment (with a prefix of FOQ)
    // E.g. `FOQ_DEBUG=1 ./target/app` would set the `debug` key
    .add_source(config::Environment::with_prefix("FOQ"))
    .build()
    .unwrap()
});

/// The certificates loaded from a file
pub(crate) static CERTS: Lazy<Vec<Certificate>> = Lazy::new(|| {
  load_certs(Path::new(
    &CONFIG
      .get_string("certificate_file")
      .expect("Certificate must be supplied in config!"),
  ))
  .expect("Unable to load certificates! Cannot start.")
});

/// The key loaded from a file
pub(crate) static KEY: Lazy<PrivateKey> = Lazy::new(|| {
  load_keys(Path::new(
    &CONFIG
      .get_string("key_file")
      .expect("Key must be supplied in config!"),
  ))
  .expect("Unable to load keys! Cannot start.")
  .first()
  .unwrap()
  .clone()
});

/// The SQLite connection
pub(crate) static DB_LAZY: Lazy<SqlitePool> = Lazy::new(|| {
  let db_url = CONFIG
    .get_string("DATABASE_URL")
    .expect("DATABASE_URL must be set!");
  info!("DATABASE_URL: {db_url}");
  SqlitePool::connect_lazy(&db_url).unwrap()
});

pub(crate) static TLS_ACCEPTOR: Lazy<Option<TlsAcceptor>> = Lazy::new(|| {
  let mut config = match rustls::ServerConfig::builder()
    .with_safe_defaults()
    .with_no_client_auth()
    .with_single_cert(CERTS.clone(), KEY.clone())
    .map_err(|err| Error::new(ErrorKind::InvalidInput, err))
  {
    Ok(c) => c,
    Err(e) => {
      warn!("Failed to configure TLS config! {e}");
      return None;
    }
  };
  config.alpn_protocols = vec!["ftp".as_bytes().to_vec()];
  if let Ok(tick) = Ticketer::new() {
    config.session_storage = ServerSessionMemoryCache::new(256);
    config.ticketer = tick;
  }

  if std::env::var_os("SSLKEYLOGFILE").is_some() {
    config.key_log = Arc::new(rustls::KeyLogFile::new());
  }
  Some(TlsAcceptor::from(Arc::new(config)))
});

pub(crate) static AUTH_PROVIDER: OnceCell<AuthProvider> = OnceCell::const_new();
