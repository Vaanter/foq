//! A form containing the username and password used to authenticate a user.

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash, Default)]
pub(crate) struct LoginForm {
  pub(crate) username: Option<String>,
  pub(crate) password: Option<String>,
}
