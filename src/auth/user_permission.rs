//! Available permissions a user can have.

use strum_macros::{EnumIter, EnumMessage, EnumString};

use crate::io::entry_data::EntryType;

#[derive(
  Copy, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq, EnumMessage, EnumIter, EnumString,
)]
#[strum(ascii_case_insensitive)]
pub(crate) enum UserPermission {
  #[strum(serialize = "r")]
  Read,
  #[strum(serialize = "w")]
  Write,
  #[strum(serialize = "a")]
  Append,
  #[strum(serialize = "c")]
  Create,
  #[strum(serialize = "e")]
  Execute,
  #[strum(serialize = "f")]
  Rename,
  #[strum(serialize = "l")]
  List,
  #[strum(serialize = "d")]
  Delete,
}

impl UserPermission {
  pub(crate) fn get_applicable_permissions(entry_type: &EntryType) -> Vec<UserPermission> {
    match entry_type {
      EntryType::File => Vec::from([
        UserPermission::Read,
        UserPermission::Write,
        UserPermission::Append,
        UserPermission::Rename,
        UserPermission::Delete,
      ]),
      EntryType::Dir | EntryType::Cdir | EntryType::Pdir => Vec::from([
        UserPermission::Create,
        UserPermission::Execute,
        UserPermission::Rename,
        UserPermission::List,
        UserPermission::Delete,
      ]),
      EntryType::Link => Vec::from([UserPermission::Delete, UserPermission::Rename]),
    }
  }
}

#[cfg(test)]
mod test {
  use strum::{EnumMessage, IntoEnumIterator};

  use crate::auth::user_permission::UserPermission;

  #[test]
  fn ensure_all_permissions_have_serialisation_test() {
    UserPermission::iter().for_each(|p| assert!(!p.get_serializations().is_empty()));
  }

  #[test]
  fn pvals_test() {
    let perm = UserPermission::Read;
    assert_eq!("r", perm.get_serializations()[0]);

    let perm = UserPermission::Append;
    assert_eq!("a", perm.get_serializations()[0]);

    let perm = UserPermission::Write;
    assert_eq!("w", perm.get_serializations()[0]);

    let perm = UserPermission::Create;
    assert_eq!("c", perm.get_serializations()[0]);

    let perm = UserPermission::Delete;
    assert_eq!("d", perm.get_serializations()[0]);

    let perm = UserPermission::Rename;
    assert_eq!("f", perm.get_serializations()[0]);

    let perm = UserPermission::List;
    assert_eq!("l", perm.get_serializations()[0]);

    let perm = UserPermission::Execute;
    assert_eq!("e", perm.get_serializations()[0]);
  }
}
