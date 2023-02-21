use std::error::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use crate::io::session::Session;
use crate::io::connection_mode::ConnectionMode;

pub(crate) trait ConnectionHandler {
  fn send_control_message(&self, message: String) -> Result<(), Box<dyn Error>> where Self: Sized;
  fn get_data_stream<T: AsyncRead + AsyncWrite>(&self, mode: ConnectionMode) -> Result<T, Box<dyn Error>> where Self: Sized;
  fn get_session(&self) -> &Session where Self: Sized;
}