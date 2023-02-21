use std::collections::HashMap;
use std::path::PathBuf;

pub(crate) struct UserData {
  username: String,
  acl: HashMap<PathBuf, bool>
}