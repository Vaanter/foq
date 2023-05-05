use std::collections::HashSet;
use std::fmt::Debug;
use std::fs::Metadata;
use std::io::Error;
use std::time::SystemTime;

use chrono::format::{DelayedFormat, StrftimeItems};
use chrono::{DateTime, Local};
use strum::EnumMessage;
use strum_macros::Display;

use crate::auth::user_permission::UserPermission;

#[allow(unused)]
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
  modify: String,
  name: String,
}

#[allow(unused)]
impl EntryData {
  pub(crate) fn new(
    size: u64,
    entry_type: EntryType,
    perm: Vec<UserPermission>,
    modify: impl Into<String>,
    name: impl Into<String>,
  ) -> Self {
    let mut modify = modify.into();
    if modify.is_empty() {
      let new_modify: DateTime<Local> = SystemTime::now().into();
      modify = new_modify.format("%Y%m%d%H%M%S").to_string();
    }
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
    let modify: DateTime<Local> = metadata.modified().unwrap_or(SystemTime::now()).into();
    let modify_formatted: DelayedFormat<StrftimeItems> = modify.format("%Y%m%d%H%M%S");

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

    Ok(EntryData::new(
      size,
      entry_type,
      permissions,
      modify_formatted.to_string(),
      name,
    ))
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
  pub fn modify(&self) -> &str {
    &self.modify
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
