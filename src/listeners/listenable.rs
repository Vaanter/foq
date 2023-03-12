use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

pub(crate) trait Listenable<T> {
    fn listen(addr: SocketAddr) -> Result<Arc<Mutex<T>>, Box<dyn Error>>
    where
        Self: Sized;
    fn stop_listening(&self) -> Result<(), Box<dyn Error>>;
}
