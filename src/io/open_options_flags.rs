//! Wraps the options in [`OpenOptions`] to allow access to the options.

use derive_builder::Builder;
use tokio::fs::OpenOptions;

/// Available options when opening a file.
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Default, Debug, Builder)]
#[builder(default)]
pub(crate) struct OpenOptionsWrapper {
  pub read: bool,
  pub write: bool,
  pub append: bool,
  pub create: bool,
  pub truncate: bool,
}

impl From<OpenOptionsWrapper> for OpenOptions {
  fn from(value: OpenOptionsWrapper) -> Self {
    let mut options = OpenOptions::new();
    options.read(value.read);
    options.write(value.write);
    options.append(value.append);
    options.create(value.create);
    options.truncate(value.truncate);
    options
  }
}
