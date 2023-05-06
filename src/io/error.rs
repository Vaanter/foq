use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum IoError {
  #[error("User not logged in!")]
  UserError,
  #[error("{0}")]
  InvalidPathError(String),
  #[error("{0}")]
  NotFoundError(String),
  #[error("OS error occurred! {0}")]
  OsError(#[from] io::Error),
  #[error("System error occurred!")]
  SystemError,
  #[error("Not a directory!")]
  NotADirectoryError,
  #[error("Not a file!")]
  NotAFileError,
  #[error("Insufficient permissions!")]
  PermissionError,
}
