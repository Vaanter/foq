use std::error::Error;
use std::path::PathBuf;

use crate::auth::user_data::UserData;
use crate::handlers::connection_handler::ConnectionHandler;
use crate::io::data_type::DataType;
use crate::io::transfer_mode::TransferMode;

pub(crate) struct Session {
  cwd: PathBuf,
  mode: TransferMode,
  data_type: DataType,
  user_data: UserData,
  connection_handler: Box<dyn ConnectionHandler + Sync + Send>,
}

impl Session {
  pub(crate) fn evaluate(&self, message: String) {}
}
