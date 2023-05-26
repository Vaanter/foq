//! An entity containing user information.

use crate::io::file_system_view::FileSystemView;

#[derive(Clone, Debug)]
pub(crate) struct UserData {
  pub(crate) username: String,
  #[allow(unused)]
  pub(crate) password: String,
  pub(crate) file_system_views: Vec<FileSystemView>,
}

impl UserData {
  /// Constructs a new [`UserData`] instance.
  ///
  /// # Arguments
  /// - `username`: A type that can be converted into a [`String`], representing the users name.
  /// - `password`: A type that can be converted into a [`String`], representing the users
  /// password.
  ///
  /// # Returns
  ///
  /// A new [`UserData`] entity.
  ///
  pub(crate) fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
    UserData {
      username: username.into(),
      password: password.into(),
      file_system_views: Vec::new(),
    }
  }

  pub(crate) fn add_view(&mut self, view: FileSystemView) {
    self.file_system_views.push(view);
  }

  #[allow(unused)]
  pub(crate) fn remove_view(&mut self, view: &FileSystemView) {
    if let Some(pos) = self.file_system_views.iter().position(|x| *x == *view) {
      self.file_system_views.remove(pos);
    }
  }
}
