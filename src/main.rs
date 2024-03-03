use std::fs::OpenOptions;
use std::str::FromStr;

use tracing::Level;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};

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
/// After the tracing is set up, the [`runner`] is executed.
///
/// [`INFO`]: Level::INFO
/// [`runner`]: runner
///
#[tokio::main]
async fn main() {
  let mut log_file_options = OpenOptions::new();
  log_file_options.write(true).truncate(true).create(true);
  let log_file = log_file_options
    .open("foq.log")
    .expect("Log file should be accessible");
  let (non_blocking, _guard) = tracing_appender::non_blocking(log_file);
  let log_level =
    Level::from_str(&CONFIG.get_string("log_level").unwrap_or_default()).unwrap_or(Level::INFO);
  let fmt_layer = tracing_subscriber::fmt::Layer::default()
    .with_writer(non_blocking)
    .with_file(false)
    .with_ansi(false)
    .with_line_number(false)
    .with_thread_ids(true)
    .with_target(false)
    .with_filter(EnvFilter::new(format!("foq={}", log_level)));

  Registry::default().with(fmt_layer).init();

  runner::run().await;
}
