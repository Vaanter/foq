use std::collections::BTreeMap;
use std::path::PathBuf;

pub(crate) struct UserData {
    pub(crate) username: String,
    pub(crate) acl: BTreeMap<PathBuf, bool>,
}

impl UserData {
    pub(crate) fn new(username: String, acl: BTreeMap<PathBuf, bool>) -> Self {
        UserData { username, acl }
    }
}
