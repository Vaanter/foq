use std::collections::BTreeMap;

use crate::io::file_system_view::FileSystemView;

#[derive(Clone, Debug)]
pub(crate) struct UserData {
  pub(crate) username: String,
  pub(crate) password: String,
  pub(crate) file_system_views: BTreeMap<String, FileSystemView>,
}

impl UserData {
  pub(crate) fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
    UserData {
      username: username.into(),
      password: password.into(),
      file_system_views: BTreeMap::new(),
    }
  }

  pub(crate) fn add_view(&mut self, view: FileSystemView) {
    self.file_system_views.insert(view.label.clone(), view);
  }

  pub(crate) fn remove_view(&mut self, label: impl Into<String>) {
    self.file_system_views.remove(&label.into());
  }

  pub(crate) fn get_view_by_label(
    &self,
    label: impl Into<String>,
  ) -> Option<&FileSystemView> {
    self.file_system_views.get(&label.into())
  }

  pub(crate) fn get_views_labels(&self) -> Vec<(&String, &FileSystemView)> {
    self.file_system_views.iter().collect()
  }
}
