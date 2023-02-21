use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

pub(crate) trait Listenable {
  fn listen(addr: SocketAddr) -> Result<Arc<Mutex<dyn Listenable>>, Box<dyn Error>> where Self: Sized;
  fn stop_listening(&self) -> Result<(), Box<dyn Error>>;
}
