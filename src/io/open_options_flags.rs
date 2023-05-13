use derive_builder::Builder;
use tokio::fs::OpenOptions;

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Default, Debug, Builder)]
#[builder(default)]
pub(crate) struct OpenOptionsWrapper {
  read: bool,
  write: bool,
  append: bool,
  create: bool,
  truncate: bool,
}

impl OpenOptionsWrapper {
  pub fn read(&self) -> bool {
    self.read
  }
  pub fn write(&self) -> bool {
    self.write
  }
  pub fn append(&self) -> bool {
    self.append
  }
  pub fn create(&self) -> bool {
    self.create
  }
  pub fn truncate(&self) -> bool {
    self.truncate
  }
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
