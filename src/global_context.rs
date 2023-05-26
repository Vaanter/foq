//! Contains global statics

use std::path::Path;

use config::Config;
use once_cell::sync::Lazy;
use rustls::{Certificate, PrivateKey};
use sqlx::SqlitePool;
use tokio::sync::OnceCell;
use tracing::info;

use crate::auth::auth_provider::AuthProvider;
use crate::utils::tls_utils::{load_certs, load_keys};

/// The configuration loaded from config file
pub(crate) static CONFIG: Lazy<Config> = Lazy::new(|| {
  Config::builder()
    .add_source(config::File::with_name("config.toml"))
    // Add in settings from the environment (with a prefix of FOQ)
    // Eg.. `FOQ_DEBUG=1 ./target/app` would set the `debug` key
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
  .get(0)
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

pub(crate) static AUTH_PROVIDER: OnceCell<AuthProvider> = OnceCell::const_new();
