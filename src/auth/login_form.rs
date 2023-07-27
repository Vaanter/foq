//! A form containing the username and password used to authenticate a user.

use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash, Default, Zeroize, ZeroizeOnDrop)]
pub(crate) struct LoginForm {
  pub(crate) username: Option<String>,
  pub(crate) password: Option<String>,
}
