//! Possible representations of data.
//!
//! Only the binary type is implemented although the ASCII type is required by
//! [RFC959](https://datatracker.ietf.org/doc/html/rfc959).

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
