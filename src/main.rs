use std::fs::OpenOptions;
use std::str::FromStr;

use tracing::Level;

use crate::global_context::CONFIG;

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
  let mut log_file_options = OpenOptions::new();
  log_file_options
    .write(true)
    .append(true)
    .truncate(false)
    .create(true);
  let log_file = log_file_options
    .open("foq.log")
    .expect("Log file should be accessible");
  let (non_blocking, _guard) = tracing_appender::non_blocking(log_file);
  let log_level = Level::from_str(&CONFIG.get_string("log_level").unwrap_or(String::new()))
    .unwrap_or(Level::INFO);
  let subscriber = tracing_subscriber::fmt()
    .with_writer(non_blocking)
    .with_env_filter(format!("foq={}", log_level))
    .with_file(false)
    .with_ansi(false)
    .with_line_number(false)
    .with_thread_ids(true)
    .with_target(false)
    .finish();
  tracing::subscriber::set_global_default(subscriber).unwrap();

  runner::run().await;
}
