//! Contains global statics

use std::io::{Error, ErrorKind};
use std::path::Path;
use std::sync::Arc;

use config::Config;
use once_cell::sync::Lazy;
use rustls::ServerConfig;
use rustls::crypto::CryptoProvider;
use rustls::crypto::aws_lc_rs::Ticketer;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::ServerSessionMemoryCache;
use sqlx::SqlitePool;
use tokio::sync::OnceCell;
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
pub(crate) static CERTS: Lazy<Vec<CertificateDer>> = Lazy::new(|| {
  load_certs(Path::new(
    &CONFIG.get_string("certificate_file").expect("Certificate must be supplied in config!"),
  ))
  .expect("Unable to load certificates! Cannot start.")
});

/// The key loaded from a file
pub(crate) static KEY: Lazy<PrivateKeyDer> = Lazy::new(|| {
  load_keys(Path::new(&CONFIG.get_string("key_file").expect("Key must be supplied in config!")))
    .expect("Unable to load keys! Cannot start.")
    .first()
    .expect("At least one key expected")
    .clone_key()
});

/// The SQLite connection
pub(crate) static DB_LAZY: Lazy<SqlitePool> = Lazy::new(|| {
  let db_url = CONFIG.get_string("DATABASE_URL").expect("DATABASE_URL must be set!");
  info!("DATABASE_URL: {db_url}");
  SqlitePool::connect_lazy(&db_url).unwrap()
});

pub(crate) static TLS_CONFIG: Lazy<Option<Arc<ServerConfig>>> = Lazy::new(|| {
  if CryptoProvider::get_default().is_none() {
    rustls::crypto::aws_lc_rs::default_provider()
      .install_default()
      .expect("CryptoProvider should install successfuly");
  }
  let mut config = match ServerConfig::builder()
    .with_no_client_auth()
    .with_single_cert(CERTS.clone(), KEY.clone_key())
    .map_err(|err| Error::new(ErrorKind::InvalidInput, err))
  {
    Ok(c) => c,
    Err(e) => {
      warn!("Failed to configure TLS config! {e}");
      return None;
    }
  };
  config.alpn_protocols = vec!["ftpoq-1".as_bytes().to_vec()];
  if let Ok(tick) = Ticketer::new() {
    config.session_storage = ServerSessionMemoryCache::new(256);
    config.ticketer = tick;
  }

  if std::env::var_os("SSLKEYLOGFILE").is_some() {
    config.key_log = Arc::new(rustls::KeyLogFile::new());
  }
  Some(Arc::new(config))
});

pub(crate) static AUTH_PROVIDER: OnceCell<AuthProvider> = OnceCell::const_new();
