use std::str::FromStr;

use tracing::Level;

use crate::global_context::{CONFIG};

mod auth;
mod commands;
mod data_channels;
mod global_context;
mod handlers;
mod io;
mod listeners;
mod runner;
mod session;
mod utils;

/// Entrypoint of the application. Runs on tokio.
///
/// # Tracing  setup
/// Attempts to load desired log level from configuration. If config does not specify this setting
/// then [`INFO`] is assumed. Then sets stdout as the tracing output.
///
/// # Runner
/// After the tracing is setup, the [`runner`] is executed.
///
/// [`INFO`]: Level::INFO
/// [`runner`]: runner
///
#[tokio::main]
async fn main() {
  let log_level = Level::from_str(&CONFIG.get_string("log_level").unwrap_or(String::new()))
    .unwrap_or(Level::INFO);
  let subscriber = tracing_subscriber::fmt()
    .with_file(false)
    .with_line_number(false)
    .with_thread_ids(true)
    .with_target(false)
    .with_max_level(log_level)
    .finish();
  tracing::subscriber::set_global_default(subscriber).unwrap();

  runner::run().await;
}
