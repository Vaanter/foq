use once_cell::sync::Lazy;
use sqlx::SqlitePool;
use tokio::sync::OnceCell;

use crate::auth::auth_provider::AuthProvider;

pub(crate) static DB_LAZY: Lazy<SqlitePool> = Lazy::new(|| {
  let db_url = dotenvy::var("DATABASE_URL").expect("");
  SqlitePool::connect_lazy(&db_url).unwrap()
});

pub(crate) static AUTH_PROVIDER: OnceCell<AuthProvider> = OnceCell::const_new();