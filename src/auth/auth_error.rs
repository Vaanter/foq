//! Various errors that can result during user authentication.

use strum_macros::Display;
use thiserror::Error;

#[derive(Debug, Eq, PartialEq, Display, Error)]
pub(crate) enum AuthError {
  UserNotFoundError,
  InvalidCredentials,
  PermissionParsingError,
  BackendError,
}
