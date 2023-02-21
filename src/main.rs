mod commands;
mod listeners;
mod handlers;
mod io;
mod auth;

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() {
}
