use once_cell::sync::Lazy;
use sqlx::SqlitePool;
use tokio::sync::OnceCell;
use tracing::info;

use crate::auth::auth_provider::AuthProvider;

pub(crate) static DB_LAZY: Lazy<SqlitePool> = Lazy::new(|| {
  let db_url = dotenvy::var("DATABASE_URL").expect("DATABASE_URL must be set!");
  info!("DATABASE_URL: {db_url}");
  SqlitePool::connect_lazy(&db_url).unwrap()
});

pub(crate) static AUTH_PROVIDER: OnceCell<AuthProvider> = OnceCell::const_new();