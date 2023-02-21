use std::error::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use crate::handlers::connection_handler::ConnectionHandler;
use crate::io::connection_mode::ConnectionMode;
use crate::io::session::Session;

pub(crate) struct StandardConnectionHandler {
  stream: TcpStream,
  session: Session,
}

impl ConnectionHandler for StandardConnectionHandler {
  fn send_control_message(&self,  message: String) -> Result<(), Box<dyn Error>> {
    todo!()
  }

  fn get_data_stream<T: AsyncRead + AsyncWrite>(&self,  mode: ConnectionMode) -> Result<T, Box<dyn Error>> {
    todo!()
  }

  fn get_session(&self) -> &Session {
    &self.session
  }
}