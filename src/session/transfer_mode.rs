#[allow(unused)]
#[derive(Copy, Clone, Debug, Default)]
pub(crate) enum TransferMode {
  #[default]
  STREAM,
  BLOCK,
  COMPRESS,
}
