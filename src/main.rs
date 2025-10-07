use chrono::Local;
use std::fs::OpenOptions;
use std::str::FromStr;
use tracing::{Level, debug, trace};
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
fn main() {
  let log_file_name = format_log_file_name();
  let mut log_file_options = OpenOptions::new();
  log_file_options.write(true).truncate(true).create(true);
  let log_file = log_file_options.open(log_file_name).expect("Log file should be accessible");
  let (non_blocking, _guard) = tracing_appender::non_blocking(log_file);
  let log_filter = CONFIG.get_string("log_filter").unwrap_or_else(|_| {
    let log_level =
      Level::from_str(&CONFIG.get_string("log_level").unwrap_or_default()).unwrap_or(Level::INFO);
    format!("foq={}", log_level)
  });
  let fmt_layer = tracing_subscriber::fmt::Layer::default()
    .with_writer(non_blocking)
    .with_file(false)
    .with_ansi(false)
    .with_line_number(false)
    .with_thread_ids(true)
    .with_target(false)
    .with_filter(EnvFilter::new(log_filter));
  Registry::default().with(fmt_layer).init();

  let threads: i64 = CONFIG.get_int("threads").unwrap_or_else(|_| {
    std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1usize) as i64
  });
  debug!("Using {} threads", threads);

  tokio::runtime::Builder::new_multi_thread()
    .worker_threads(threads as usize)
    .enable_all()
    .on_task_spawn(|meta| {
      trace!("Task {} spawned", meta.id());
    })
    .on_task_terminate(|task| {
      trace!("Task {} terminated", task.id());
    })
    .build()
    .unwrap()
    .block_on(async {
      runner::run().await;
    });
}

fn format_log_file_name() -> String {
  let name = CONFIG.get_string("logfile").unwrap_or(String::from("foq-%Y%m%d%H%M.log"));
  let current_time = Local::now();
  current_time.format(&name).to_string()
}
