use std::collections::HashSet;
use std::fs::Metadata;
use std::io::Error;
use std::time::SystemTime;

use strum::EnumMessage;
use strum_macros::Display;

use crate::auth::user_permission::UserPermission;

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug, Display, Hash)]
#[strum(serialize_all = "lowercase")]
pub(crate) enum EntryType {
  FILE,
  DIR,
  CDIR,
  PDIR,
  LINK,
}

#[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Debug, Hash)]
pub(crate) struct EntryData {
  size: u64,
  entry_type: EntryType,
  perm: Vec<UserPermission>,
  modify: u128,
  name: String,
}

impl EntryData {
  pub(crate) fn new(
    size: u64,
    entry_type: EntryType,
    perm: Vec<UserPermission>,
    modify: u128,
    name: impl Into<String>,
  ) -> Self {
    EntryData {
      size,
      entry_type,
      perm,
      modify,
      name: name.into(),
    }
  }

  pub(crate) fn change_entry_type(&mut self, new_type: EntryType) {
    self.entry_type = new_type;
  }

  pub(crate) fn create_from_metadata(
    metadata: std::io::Result<Metadata>,
    name: impl Into<String>,
    permissions: &HashSet<UserPermission>,
  ) -> Result<Self, Error> {
    let metadata = metadata?;
    let size = metadata.len();
    let modify = metadata
      .modified()
      .unwrap_or(SystemTime::UNIX_EPOCH)
      .elapsed()
      .unwrap()
      .as_nanos();

    let entry_type = if metadata.is_file() {
      EntryType::FILE
    } else if metadata.is_dir() {
      EntryType::DIR
    } else if metadata.is_symlink() {
      EntryType::LINK
    } else {
      unreachable!();
    };

    let permissions = UserPermission::get_applicable_permissions(&entry_type)
      .iter()
      .filter_map(|p| permissions.get(p).map(|v| v.to_owned()))
      .collect();

    Ok(EntryData::new(size, entry_type, permissions, modify, name))
  }
  pub fn size(&self) -> u64 {
    self.size
  }
  pub fn entry_type(&self) -> EntryType {
    self.entry_type
  }
  pub fn perm(&self) -> &Vec<UserPermission> {
    &self.perm
  }
  pub fn modify(&self) -> u128 {
    self.modify
  }
  pub fn name(&self) -> &str {
    &self.name
  }
}

impl ToString for EntryData {
  fn to_string(&self) -> String {
    let mut buffer = String::new();
    buffer.push_str(&format!("size={};", self.size));
    buffer.push_str(&format!("type={};", self.entry_type));
    buffer.push_str(&format!("modify={};", self.modify));
    buffer.push_str(&format!(
      "perm={};",
      self
        .perm
        .iter()
        .map(|p| p.get_serializations().get(0).map(|s| *s).unwrap_or(""))
        .collect::<String>()
    ));
    buffer.push_str(&format!(" {}", self.name));
    buffer.push('\r');
    buffer.push('\n');
    buffer
  }
}
