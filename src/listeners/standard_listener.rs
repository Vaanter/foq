use std::error::Error;
use std::net::SocketAddr;
use crate::handlers::standard_connection_handler::StandardConnectionHandler;

use tokio::net::TcpListener;
use std::net::TcpListener as StdTcpListener;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::listeners::listenable::Listenable;

pub(crate) struct StandardListener {
  listener: TcpListener,
  handlers: Vec<StandardConnectionHandler>,
}

impl Listenable for StandardListener {
  fn listen(addr: SocketAddr) -> Result<Arc<Mutex<dyn Listenable>>, Box<dyn Error>> where Self: Sized {
    let listener = Arc::from(Mutex::from(StandardListener {
      listener: TcpListener::from_std(StdTcpListener::bind(addr)?)?,
      handlers: vec![],
    }));

    {
      let listener = listener.clone();
      tokio::spawn(async move {
        println!("Listening on TCP!");
        listener.lock().await.listener.accept().await.unwrap();
        println!("No longer listening on TCP!");
      });
    }

    Ok(listener)
  }

  fn stop_listening(&self) -> Result<(), Box<dyn Error>> {
    todo!()
  }
}