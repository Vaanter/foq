use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum Error {
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
}
