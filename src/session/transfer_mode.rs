//! Possible modes of tranfering data.
//!
//! Only the STREAM mode is implemented and used.

#[allow(unused)]
#[derive(Copy, Clone, Debug, Default)]
pub(crate) enum TransferMode {
  #[default]
  STREAM,
  BLOCK,
  COMPRESS,
}
