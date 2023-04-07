#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash, Default)]
pub(crate) struct LoginForm {
  pub(crate) username: Option<String>,
  pub(crate) password: Option<String>,
}

