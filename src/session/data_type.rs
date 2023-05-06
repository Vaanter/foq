#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub(crate) enum DataType {
  ASCII { sub_type: SubType },
  BINARY,
}

impl Default for DataType {
  fn default() -> Self {
    DataType::ASCII {
      sub_type: SubType::default(),
    }
  }
}

#[derive(Copy, Clone, Debug, Default, Ord, PartialOrd, Eq, PartialEq)]
pub(crate) enum SubType {
  #[default]
  NonPrint,
  TelnetFormatEffectors,
  CarriageControl,
}
