//! Various errors that can result from I/O operations.

use std::io;
use std::io::{Error, ErrorKind};
use thiserror::Error;
use tracing::debug;

#[allow(clippy::enum_variant_names)]
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

impl IoError {
  pub(crate) fn map_io_error(error: Error) -> IoError {
    debug!("Mapping error: {:#?}", error);
    match error.kind() {
      ErrorKind::NotFound => IoError::NotFoundError(error.to_string()),
      ErrorKind::PermissionDenied => IoError::PermissionError,
      _ => IoError::OsError(error),
    }
  }
}
