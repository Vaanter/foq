use std::error::Error;
use s2n_quic::Connection;
use tokio::io::{AsyncRead, AsyncWrite};
use crate::handlers::connection_handler::ConnectionHandler;
use crate::io::connection_mode::ConnectionMode;
use crate::io::session::Session;

pub(crate) struct QuicOnlyConnectionHandler {
  connection: Connection,
  session: Session,
}

impl ConnectionHandler for QuicOnlyConnectionHandler {
  fn send_control_message(&self, message: String) -> Result<(), Box<dyn Error>> {
    todo!()
  }

  fn get_data_stream<T: AsyncRead + AsyncWrite>(&self, mode: ConnectionMode) -> Result<T, Box<dyn Error>> {
    todo!()
  }

  fn get_session(&self) -> &Session {
    &self.session
  }
}