use std::str::FromStr;

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum ProtMode {
  #[default]
  Clear,
  Safe,
  Confidential,
  Private,
}

impl FromStr for ProtMode {
  type Err = ();

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "C" | "c" => Ok(ProtMode::Clear),
      "S" | "s" => Ok(ProtMode::Safe),
      "E" | "e" => Ok(ProtMode::Confidential),
      "P" | "p" => Ok(ProtMode::Private),
      _ => Err(()),
    }
  }
}
