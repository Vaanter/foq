//! Possible modes of transferring data.
//!
//! Only the STREAM mode is implemented and used.

#[allow(unused)]
#[derive(Copy, Clone, Debug, Default)]
pub(crate) enum TransferMode {
  #[default]
  Stream,
  Block,
  Compress,
}
