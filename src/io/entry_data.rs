//! Information about an object in the filesystem, such as file or directory.

use std::collections::HashSet;
use std::fmt::Debug;
use std::fs::Metadata;
use std::io;
use std::io::Error;
use std::time::SystemTime;

use chrono::format::{DelayedFormat, StrftimeItems};
use chrono::{DateTime, Local};
use strum::EnumMessage;
use strum_macros::Display;

use crate::auth::user_permission::UserPermission;

/// Entry type fact as specified by
/// [RFC3659](https://datatracker.ietf.org/doc/html/rfc3659#section-7.5.1).
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

const MLSD_DATETIME_FORMAT: &'static str = "%Y%m%d%H%M%S";
const LIST_DATETIME_FORMAT_TIME: &'static str = "%b %d %H:%M";
const LIST_DATETIME_FORMAT_YEAR: &'static str = "%b %d %Y";

/// Holds the various facts about a filesystem object.
#[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Debug, Hash)]
pub(crate) struct EntryData {
  size: u64,
  entry_type: EntryType,
  perm: Vec<UserPermission>,
  modify: SystemTime,
  name: String,
}

#[allow(unused)]
impl EntryData {
  /// Constructs a new entry.
  ///
  /// If the modify fact is not set, then the current system time is assumed. If it is set then
  /// care must be taken to assure it's in the correct format as this function does no checks.
  pub(crate) fn new(
    size: u64,
    entry_type: EntryType,
    perm: Vec<UserPermission>,
    modify: SystemTime,
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

  /// Constructs a new entry from the metadata of an object.
  ///
  /// Users permissions are filtered to permissions that are relevant for an object.
  pub(crate) fn create_from_metadata(
    metadata: io::Result<Metadata>,
    name: impl Into<String>,
    permissions: &HashSet<UserPermission>,
  ) -> Result<Self, Error> {
    let metadata = metadata?;
    let size = metadata.len();

    let modify = metadata.modified().unwrap_or(SystemTime::now());

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
  pub fn modify(&self) -> &SystemTime {
    &self.modify
  }
  pub fn name(&self) -> &str {
    &self.name
  }

  pub(crate) fn to_list_string(&self) -> String {
    let mut buffer = String::with_capacity(64);
    let type_str = match self.entry_type {
      EntryType::FILE => "-",
      EntryType::DIR | EntryType::CDIR | EntryType::PDIR => "d",
      EntryType::LINK => "l",
    };
    buffer.push_str(type_str);
    let mut perm_str = ["-", "-", "-"];
    if self.perm.contains(&UserPermission::READ) {
      perm_str[0] = "r";
    }
    if self.perm.contains(&UserPermission::WRITE) {
      perm_str[1] = "w";
    }
    if self.perm.contains(&UserPermission::EXECUTE) {
      perm_str[2] = "x";
    }
    buffer.push_str(&perm_str.repeat(3).join(""));
    buffer.push(' ');

    buffer.push('1');
    buffer.push(' ');

    let owners = ["user", "group"];
    buffer.push_str(&owners.join(" "));
    buffer.push(' ');

    buffer.push_str(&format!("{:>13}", self.size));
    buffer.push(' ');

    let modify_dt: DateTime<Local> = self.modify.into();
    let modify_formatted: DelayedFormat<StrftimeItems> =
      if Local::now().signed_duration_since(modify_dt).num_days() > 180 {
        modify_dt.format(LIST_DATETIME_FORMAT_YEAR)
      } else {
        modify_dt.format(LIST_DATETIME_FORMAT_TIME)
      };

    buffer.push_str(&modify_formatted.to_string());
    buffer.push(' ');

    buffer.push_str(&self.name);
    buffer.push('\r');
    buffer.push('\n');
    buffer
  }
}

impl ToString for EntryData {
  fn to_string(&self) -> String {
    let mut buffer = String::new();
    let modify_dt: DateTime<Local> = self.modify.into();
    let modify_formatted: DelayedFormat<StrftimeItems> = modify_dt.format(MLSD_DATETIME_FORMAT);
    buffer.push_str(&format!("size={};", self.size));
    buffer.push_str(&format!("type={};", self.entry_type));
    buffer.push_str(&format!("modify={};", modify_formatted));
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
