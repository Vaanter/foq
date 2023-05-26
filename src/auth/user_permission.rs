//! Available permissions a user can have.

use strum_macros::{EnumIter, EnumMessage, EnumString};

use crate::io::entry_data::EntryType;

#[derive(Copy, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq, EnumMessage, EnumIter, EnumString)]
#[strum(ascii_case_insensitive)]
pub(crate) enum UserPermission {
  #[strum(serialize = "r")]
  READ,
  #[strum(serialize = "w")]
  WRITE,
  #[strum(serialize = "a")]
  APPEND,
  #[strum(serialize = "c")]
  CREATE,
  #[strum(serialize = "e")]
  EXECUTE,
  #[strum(serialize = "f")]
  RENAME,
  #[strum(serialize = "l")]
  LIST,
  #[strum(serialize = "d")]
  DELETE,
}

impl UserPermission {
  pub(crate) fn get_applicable_permissions(entry_type: &EntryType) -> Vec<UserPermission> {
    match entry_type {
      EntryType::FILE => Vec::from([
        UserPermission::READ,
        UserPermission::WRITE,
        UserPermission::APPEND,
        UserPermission::RENAME,
        UserPermission::DELETE,
      ]),
      EntryType::DIR | EntryType::CDIR | EntryType::PDIR => Vec::from([
        UserPermission::CREATE,
        UserPermission::EXECUTE,
        UserPermission::RENAME,
        UserPermission::LIST,
        UserPermission::DELETE,
      ]),
      EntryType::LINK => Vec::from([UserPermission::DELETE, UserPermission::RENAME]),
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
    let perm = UserPermission::READ;
    assert_eq!("r", perm.get_serializations()[0]);

    let perm = UserPermission::APPEND;
    assert_eq!("a", perm.get_serializations()[0]);

    let perm = UserPermission::WRITE;
    assert_eq!("w", perm.get_serializations()[0]);

    let perm = UserPermission::CREATE;
    assert_eq!("c", perm.get_serializations()[0]);

    let perm = UserPermission::DELETE;
    assert_eq!("d", perm.get_serializations()[0]);

    let perm = UserPermission::RENAME;
    assert_eq!("f", perm.get_serializations()[0]);

    let perm = UserPermission::LIST;
    assert_eq!("l", perm.get_serializations()[0]);

    let perm = UserPermission::EXECUTE;
    assert_eq!("e", perm.get_serializations()[0]);
  }
}
