extern crate core;

use crate::lab::run;

mod auth;
mod commands;
mod handlers;
mod io;
mod lab;
mod listeners;
mod utils;

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() {
    run().await;
}
