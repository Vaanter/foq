//! File system view root represent the start of the directory tree an user can access.
//! See [`File system view`] module documentation.
//!
//! [`File system view`]: crate::io::file_system_view

use crate::auth::user_permission::UserPermission;
use crate::io::entry_data::{EntryData, EntryType};
use crate::io::error::IoError;
use crate::io::open_options_flags::{OpenOptionsWrapper, OpenOptionsWrapperBuilder};
use crate::io::view::View;
use crate::io::view_dispatch::ViewDispatch;
use std::collections::HashMap;
use std::fs::FileTimes;
use std::time::SystemTime;
use tokio::fs::File;
use tracing::{debug, trace};
use tracing_attributes::instrument;
use unicode_segmentation::UnicodeSegmentation;

/// Contains the users file system views.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct FileSystemViewRoot {
  pub(crate) file_system_views: Option<HashMap<String, ViewDispatch>>,
  current_view: Option<String>,
}

enum ViewType<'a> {
  Virtual(&'a FileSystemViewRoot),
  Real(&'a ViewDispatch),
}

/// Permissions a user can have in the root.
const ROOT_PERMISSIONS: [UserPermission; 2] = [UserPermission::Execute, UserPermission::List];

impl FileSystemViewRoot {
  /// Constructs a new instance of [`FileSystemViewRoot`].
  #[cfg(test)]
  pub(crate) fn new(views: Option<HashMap<String, ViewDispatch>>) -> Self {
    FileSystemViewRoot {
      file_system_views: views,
      current_view: None,
    }
  }

  pub(crate) fn set_views(&mut self, view: Vec<ViewDispatch>) {
    let views = view.into_iter().map(|v| (v.get_label().to_string(), v)).collect();
    self.file_system_views = Some(views);
  }

  /// Changes the current path to the specified one.
  ///
  /// This function changes the current path to `path` and returns [`Ok`] if the new path is valid,
  /// [`Err(IoError)`] otherwise. New path can be absolute or relative and also the current path
  /// (.), parent (..) and root (/ or ~).
  ///
  /// # Arguments
  ///
  /// `path`: A type that can be converted into a `String`, that will be used to construct the new
  /// path.
  ///
  /// # Returns
  ///
  /// A [`Result`] containing **bool** if successful, or an [`IoError`] if an error occurs.
  /// If the [`Result`] is **true** then the path actually changed, i.e.: new and old paths differ.
  ///
  /// # Errors
  ///
  /// This function can return the following [`IoError`] variants:
  ///
  /// - [`IoError::UserError`]: If the user is not logged in.
  /// - [`IoError::SystemError`]: If a programmatic error occurs, e.g.: the current view_view
  ///   refers to nonexistent [`FileSystemView`].
  /// - [`IoError::InvalidPathError`]: If the `path` refers to parent (..) but current path is
  ///   already at root.
  /// - Other [`IoError`] returned by [`FileSystemView::change_working_directory`].
  ///
  pub(crate) fn change_working_directory(&mut self, path: &str) -> Result<bool, IoError> {
    if self.file_system_views.is_none() {
      return Err(IoError::UserError);
    }

    match self.find_view(path) {
      Some((ViewType::Real(v), sub_path)) => {
        let label = v.get_label().to_string();
        self.current_view.replace(label);
        if sub_path == "/" || sub_path.is_empty() {
          return Ok(true);
        }
        self
          .file_system_views
          .as_mut()
          .unwrap()
          .get_mut(self.current_view.as_ref().unwrap())
          .unwrap()
          .change_working_directory(sub_path)
      }
      Some((ViewType::Virtual(_), _)) => Ok(self.current_view.take().is_some()),
      None => {
        Err(IoError::InvalidPathError(String::from("Path is not available or inaccessible!")))
      }
    }
  }

  /// Changes current path to parent.
  ///
  /// See [`FileSystemViewRoot::change_working_directory`].
  pub(crate) fn change_working_directory_up(&mut self) -> Result<bool, IoError> {
    self.change_working_directory("..")
  }

  pub(crate) fn create_directory(&self, path: &str) -> Result<String, IoError> {
    if self.file_system_views.is_none() {
      // User is not logged in
      return Err(IoError::UserError);
    }

    match self.find_view(path) {
      Some((ViewType::Virtual(_), _)) => Err(IoError::SystemError),
      Some((ViewType::Real(v), sub_path)) => v.create_directory(sub_path),
      None => Err(IoError::InvalidPathError(String::from("Directory path is invalid!"))),
    }
  }

  /// Returns the path to current working directory.
  #[instrument(skip(self))]
  pub(crate) fn get_current_working_directory(&self) -> String {
    debug!("Getting current working directory path");
    if self.current_view.is_none() || self.file_system_views.is_none() {
      return String::from("/");
    }

    let current_view = self.current_view.as_ref().unwrap();
    self
      .file_system_views
      .as_ref()
      .unwrap()
      .get(current_view)
      .unwrap()
      .get_display_path()
      .to_string()
  }

  /// Creates a directory listing.
  ///
  /// This function lists all files and directories at `path` as [`EntryData`]. If the listing
  /// succeeds, then it is returned, otherwise [`IoError`] is returned.
  ///
  /// # Arguments
  ///
  /// - `path` A type that can be converted into a [`String`], representing the path to directory
  /// to list.
  ///
  /// # Returns
  ///
  /// A [`Result`] containing the listing as [`Vec<EntryData>`] if successful or an [`IoError`] if
  /// an error occurs.
  ///
  #[instrument(skip(self, path))]
  pub(crate) fn list_dir(&self, path: &str) -> Result<Vec<EntryData>, IoError> {
    if self.file_system_views.is_none() {
      // not logged in
      return Err(IoError::UserError);
    }

    if let Some((view_type, sub_path)) = self.find_view(path) {
      match view_type {
        ViewType::Virtual(v) => v.list_root(),
        ViewType::Real(v) => v.list_dir(sub_path),
      }
    } else {
      Err(IoError::InvalidPathError(String::from("Invalid path!")))
    }
  }

  /// Creates a listing of the root.
  ///
  /// This function lists all views this root contains as [`EntryData`]. If the listing
  /// succeeds, then it is returned, otherwise [`IoError`] is returned.
  ///
  /// # Returns
  ///
  /// A [`Result`] containing the listing as [`Vec<EntryData>`] if successful or an [`IoError`] if
  /// an error occurs.
  ///
  /// # Errors
  ///
  /// This function can return the following `IoError` variants:
  ///
  /// - [`IoError::UserError`]: If the user is not logged in.
  ///
  fn list_root(&self) -> Result<Vec<EntryData>, IoError> {
    if self.file_system_views.is_none() {
      return Err(IoError::UserError);
    }

    let mut entries: Vec<EntryData> =
      Vec::with_capacity(self.file_system_views.as_ref().unwrap().len() + 1);
    entries.push(EntryData::new(
      0,
      EntryType::Cdir,
      ROOT_PERMISSIONS.to_vec(),
      SystemTime::now(),
      "/",
    ));
    entries.extend(self.file_system_views.as_ref().unwrap().iter().map(|v| {
      EntryData::new(
        0,
        EntryType::Dir,
        v.1.get_permissions().iter().cloned().collect(),
        SystemTime::now(),
        v.1.get_label().to_string(),
      )
    }));

    Ok(entries)
  }

  /// Opens a file with the specified path and options.
  ///
  /// See: [`FileSystemView::open_file`].
  #[instrument(skip(self, path, options))]
  pub(crate) async fn open_file(
    &self,
    path: &str,
    options: OpenOptionsWrapper,
  ) -> Result<File, IoError> {
    if self.file_system_views.is_none() {
      return Err(IoError::UserError);
    }

    match self.find_view(path) {
      Some((ViewType::Virtual(_), _)) => {
        Err(IoError::InvalidPathError(String::from("Path references a directory, not a file!")))
      }
      Some((ViewType::Real(v), subpath)) => v.open_file(subpath, options).await,
      None => Err(IoError::UserError),
    }
  }

  #[instrument(skip(self, path))]
  pub(crate) async fn delete_file(&self, path: &str) -> Result<(), IoError> {
    if self.file_system_views.is_none() {
      return Err(IoError::UserError);
    }

    if path.is_empty() || path == "/" {
      return Err(IoError::InvalidPathError(String::from("Cannot delete root directory!")));
    }

    let view = self.find_view(path);

    if let Some((ViewType::Real(v), sub_path)) = view {
      v.delete_file(sub_path).await
    } else {
      Err(IoError::NotFoundError(String::from("Path doesn't exist!")))
    }
  }

  #[instrument(skip(self, path))]
  pub(crate) async fn delete_folder(&self, path: &str) -> Result<(), IoError> {
    if self.file_system_views.is_none() {
      return Err(IoError::UserError);
    }

    if path.is_empty() || path == "/" {
      return Err(IoError::InvalidPathError(String::from("Cannot delete root directory!")));
    }

    let view = self.find_view(path);

    if let Some((ViewType::Real(v), sub_path)) = view {
      v.delete_folder(sub_path).await
    } else {
      Err(IoError::NotFoundError(String::from("Path doesn't exist!")))
    }
  }

  #[instrument(skip(self, path))]
  pub(crate) async fn delete_folder_recursive(&self, path: &str) -> Result<(), IoError> {
    if self.file_system_views.is_none() {
      return Err(IoError::UserError);
    }

    if path.is_empty() || path == "/" {
      return Err(IoError::InvalidPathError(String::from("Cannot delete root directory!")));
    }

    let view = self.find_view(path);

    if let Some((ViewType::Real(v), sub_path)) = view {
      v.delete_folder_recursive(sub_path).await
    } else {
      Err(IoError::NotFoundError(String::from("Path doesn't exist!")))
    }
  }

  pub(crate) async fn change_file_times(
    &self,
    new_time: FileTimes,
    path: &str,
  ) -> Result<(), IoError> {
    if self.file_system_views.is_none() {
      return Err(IoError::UserError);
    }

    if path.is_empty() || path == "/" {
      return Err(IoError::InvalidPathError(String::from(
        "Cannot change modification time of current directory!",
      )));
    }

    let view = self.find_view(path);

    if let Some((ViewType::Real(v), sub_path)) = view {
      v.change_file_times(new_time, sub_path).await
    } else {
      Err(IoError::NotFoundError(String::from("Path doesn't exist!")))
    }
  }

  /// Renames a file or directory. Effectively moves an object from one path to another.
  ///
  /// This function looks up a view based on `from_path` and another one based on `to_path`.
  /// Both views must be [`ViewType::Real`] (i.e. they refer to a real file system) as objects
  /// in the virtual vies are managed by a DB and cannot be altered from outside. An object
  /// pointed to by `from_path` must exist. Object at  `to_path` will be overwritten if it exists.
  ///
  /// # Arguments
  ///
  /// - `from_path`: A source path of an object that will be renamed and thus might no longer exist
  /// afterward.
  ///  - `to_path`: A target path to which the source object will be renamed/moved.
  ///
  /// # Returns
  ///
  /// An empty [`Result`] if successful, or an [`IoError`] if an error occurs.
  ///
  /// # Errors
  ///
  /// This function can return the following [`IoError`] variants:
  ///
  /// - [`IoError::UserError`]: If the user is not logged in.
  /// - [`IoError::NotFoundError`]: If the source object does not exist.
  /// - [`IoError::InvalidPathError`]: If the source and/or the target refer to an object in a
  /// virtual view.
  /// - Other [`IoError`] mapped from [`tokio::fs::rename`].
  ///
  #[instrument(skip(self))]
  pub(crate) async fn rename_object(&self, from_path: &str, to_path: &str) -> Result<(), IoError> {
    trace!("Renaming object");
    if self.file_system_views.is_none() {
      return Err(IoError::UserError);
    }

    if from_path.is_empty() || from_path == "/" || to_path.is_empty() || to_path == "/" {
      return Err(IoError::InvalidPathError(String::from("Can not rename the root directory")));
    }

    let view_from = self.find_view(from_path);
    let view_to = self.find_view(to_path);

    match (view_from, view_to) {
      (
        Some((ViewType::Real(from_v), from_sub_path)),
        Some((ViewType::Real(to_v), to_sub_path)),
      ) => {
        if from_sub_path.is_empty() || to_sub_path.is_empty() {
          return Err(IoError::InvalidPathError(String::from("Renaming a view is not allowed")));
        }

        let rename_options = OpenOptionsWrapperBuilder::default()
          .read(true)
          .write(true)
          .truncate(true)
          .build()
          .unwrap();
        let from_pathbuf = from_v.find_path(from_sub_path, rename_options)?;
        if !from_pathbuf.exists() {
          return Err(IoError::NotFoundError(String::from("Source path does not exist")));
        }
        let to_pathbuf = to_v.find_path(to_sub_path, rename_options)?;

        debug!("Renaming - from: {:?} to {:?}", &from_pathbuf, &to_pathbuf);
        if let Err(e) = tokio::fs::rename(&from_pathbuf, &to_pathbuf).await {
          debug!("Failed to rename: {:?}", e);
          return Err(IoError::map_io_error(e));
        }

        Ok(())
      }
      (Some((ViewType::Virtual(_), _)), _) | (_, Some((ViewType::Virtual(_), _))) => {
        Err(IoError::InvalidPathError(String::from("Renaming a view is not allowed")))
      }
      (_, _) => Err(IoError::NotFoundError(String::from("Source or target path does not exist"))),
    }
  }

  #[instrument(skip(self), fields(has_views = self.file_system_views.is_some(), in_view = self.current_view.is_some()))]
  fn find_view<'a>(&self, path: &'a str) -> Option<(ViewType<'_>, &'a str)> {
    trace!("Finding view for path");
    if path == "/" || path == "~" {
      Some((ViewType::Virtual(self), ""))
    } else if path == ".." || path == "." {
      if let Some(ref label) = self.current_view {
        if let Some(v) = self.file_system_views.as_ref().unwrap().get(label) {
          if v.get_root_path() == v.get_current_path() && path == ".." {
            Some((ViewType::Virtual(self), path))
          } else {
            Some((ViewType::Real(v), path))
          }
        } else {
          None
        }
      } else if path == ".." {
        None
      } else {
        Some((ViewType::Virtual(self), "."))
      }
    } else if path.starts_with('/') || (self.current_view.is_none() && !path.is_empty()) {
      // skip the starting slash
      let offset = if path.starts_with('/') { 1 } else { 0 };
      let mut sub_path_iter = path.graphemes(true).skip(offset);
      let slash_idx = sub_path_iter
        .position(|c| {
          if c == "/" {
            return true;
          }
          false
        })
        .map(|idx| idx + offset);
      trace!("Slash index: {:?}", slash_idx);
      let view = self.file_system_views.as_ref().unwrap();
      if let Some(idx) = slash_idx {
        view.get(&path[offset..idx]).map(|v| (ViewType::Real(v), &path[idx + offset..]))
      } else {
        view.get(&path[offset..]).map(|v| (ViewType::Real(v), ""))
      }
    } else if let Some(ref label) = self.current_view {
      self.file_system_views.as_ref().unwrap().get(label).map(|v| (ViewType::Real(v), path))
    } else if path.is_empty() {
      Some((ViewType::Virtual(self), ""))
    } else {
      unreachable!("Path should have had all possibilities checked!");
    }
  }
}

#[cfg(test)]
mod tests {
  use blake3::Hasher;
  use chrono::{DateTime, Local, TimeDelta};
  use std::collections::{HashMap, HashSet};
  use std::env::temp_dir;
  use std::fs::{File, FileTimes};
  use std::ops::Sub;
  use tracing::info;
  use uuid::Uuid;

  use crate::auth::user_permission::UserPermission;
  use crate::io::entry_data::EntryData;
  use crate::io::error::IoError;
  use crate::io::file_system_view::FileSystemView;
  use crate::io::file_system_view::tests::validate_listing;
  use crate::io::file_system_view_root::FileSystemViewRoot;
  use crate::io::open_options_flags::OpenOptionsWrapperBuilder;
  use crate::io::view::View;
  use crate::io::view_dispatch::ViewDispatch;
  use crate::utils::test_utils::*;
  use crate::{tracing_error, tracing_print};

  #[tokio::test]
  async fn open_file_not_logged_in_test() {
    setup_tracing();
    let root = FileSystemViewRoot::new(None);

    let options = OpenOptionsWrapperBuilder::default().read(true).build().unwrap();

    let file = root.open_file("test_file", options).await;
    let Err(IoError::UserError) = file else {
      panic!("Expected User error");
    };
  }

  #[tokio::test]
  #[cfg(windows)]
  async fn open_file_windows_like_path_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read]);

    let mut root1 = std::env::current_dir().unwrap();
    root1.push("test_files");
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let options = OpenOptionsWrapperBuilder::default().read(true).build().unwrap();
    let mut root = FileSystemViewRoot::new(Some(views));
    root.change_working_directory(label1).unwrap();
    let file = root.open_file(&format!("{}/2KiB.txt", root1.to_str().unwrap()), options).await;
    let Err(IoError::InvalidPathError(_)) = file else {
      panic!("Expected InvalidPath Error, got: {:?}", file);
    };
  }

  #[tokio::test]
  async fn open_file_absolute_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read]);

    let mut root1 = std::env::current_dir().unwrap();
    let mut root2 = std::env::current_dir().unwrap();
    root1.push("src");
    root2.push("test_files");
    let label1 = "src";
    let label2 = "test_files";
    let view1 = FileSystemView::new(root1, label1, permissions.clone());
    let view2 = FileSystemView::new(root2, label2, permissions.clone());
    let views = create_root(vec![view1, view2]);

    let options = OpenOptionsWrapperBuilder::default().read(true).build().unwrap();
    let root = FileSystemViewRoot::new(Some(views));
    let file = root.open_file(&format!("/{label2}/2KiB.txt"), options).await;
    assert!(file.is_ok());
  }

  #[tokio::test]
  async fn open_file_relative_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read]);

    let mut root1 = std::env::current_dir().unwrap();
    let mut root2 = std::env::current_dir().unwrap();
    root1.push("src");
    root2.push("test_files");
    let label1 = "src";
    let label2 = "test_files";
    let view1 = FileSystemView::new(root1, label1, permissions.clone());
    let view2 = FileSystemView::new(root2, label2, permissions.clone());
    let views = create_root(vec![view1, view2]);

    let options = OpenOptionsWrapperBuilder::default().read(true).build().unwrap();
    let root = FileSystemViewRoot::new(Some(views));
    let file = root.open_file(&format!("{label2}/2KiB.txt"), options).await;
    assert!(file.is_ok());
  }

  #[tokio::test]
  async fn open_file_relative_nonexistent_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read]);

    let mut root1 = std::env::current_dir().unwrap();
    let mut root2 = std::env::current_dir().unwrap();
    root1.push("src");
    root2.push("test_files");
    let label1 = "src";
    let label2 = "test_files";
    let view1 = FileSystemView::new(root1, label1, permissions.clone());
    let view2 = FileSystemView::new(root2, label2, permissions.clone());
    let views = create_root(vec![view1, view2]);

    let options = OpenOptionsWrapperBuilder::default().read(true).build().unwrap();
    let root = FileSystemViewRoot::new(Some(views));
    let file = root.open_file(&format!("{label2}/NONEXISTENT"), options).await;
    let Err(IoError::NotFoundError(_)) = file else {
      panic!("Expected NotFound error, got: {:?}", file);
    };
  }

  #[test]
  fn get_cwd_not_logged_in() {
    setup_tracing();
    let root = FileSystemViewRoot::new(None);

    let cwd = root.get_current_working_directory();
    assert_eq!(String::from("/"), cwd);
  }

  #[test]
  fn get_cwd_with_view() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read]);
    let root1 = std::env::current_dir().unwrap();
    let label = "current_dir";
    let view1 = FileSystemView::new(root1.clone(), label, permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    assert!(root.change_working_directory(&format!("{}/test_files", label)).unwrap());
    assert_eq!(root.get_current_working_directory(), format!("/{}/test_files", label));
  }

  #[test]
  fn cwd_not_logged_in_test() {
    setup_tracing();
    let mut root = FileSystemViewRoot::new(None);
    let result = root.change_working_directory("/");
    let Err(IoError::UserError) = result else {
      panic!("Expected User Error, Got: {:?}", result);
    };
    assert!(root.current_view.is_none());
  }

  #[test]
  fn cwd_to_root_from_file_system_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read]);
    let root1 = std::env::current_dir().unwrap();
    let label = "current_dir";
    let view1 = FileSystemView::new(root1.clone(), label, permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    assert!(root.change_working_directory(&format!("{}/test_files", label)).unwrap());
    assert!(root.change_working_directory("~").unwrap());
    assert!(root.current_view.is_none());
  }

  #[test]
  fn cwd_to_root_from_root_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read]);
    let root1 = std::env::current_dir().unwrap();
    let label = "current_dir";
    let view1 = FileSystemView::new(root1.clone(), label, permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    let change = root.change_working_directory("~");
    let Ok(false) = change else {
      panic!("Expected Ok(false), Got: {:?}", change);
    };
    assert!(root.current_view.is_none());
  }

  #[test]
  fn cwd_to_current_from_root_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read]);
    let root1 = std::env::current_dir().unwrap();
    let label = "current_dir";
    let view1 = FileSystemView::new(root1.clone(), label, permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    let change = root.change_working_directory(".");
    let Ok(false) = change else {
      panic!("Expected Ok(false), Got: {:?}", change);
    };
    assert!(root.current_view.is_none());
  }

  #[test]
  fn cwd_to_file_system_from_root_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read]);
    let root1 = std::env::current_dir().unwrap();
    let label = "current_dir";
    let view1 = FileSystemView::new(root1.clone(), label, permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    assert!(root.change_working_directory(label).unwrap());
    assert!(root.current_view.is_some());
    assert_eq!(root.current_view.unwrap(), label);
    assert_eq!(
      root.file_system_views.unwrap().get(label).unwrap().get_display_path(),
      format!("/{label}")
    );
  }

  #[test]
  fn cwd_to_file_system_from_root_relative_multi_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read]);
    let root1 = std::env::current_dir().unwrap();
    let label = "current_dir";
    let view1 = FileSystemView::new(root1.clone(), label, permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    assert!(root.change_working_directory(&format!("{label}/test_files")).unwrap());
    assert!(root.current_view.is_some());
    assert_eq!(root.current_view.unwrap(), label);
    assert_eq!(
      root.file_system_views.unwrap().get(label).unwrap().get_display_path(),
      format!("/{label}/test_files")
    );
  }

  #[test]
  fn cwd_to_file_system_from_root_invalid_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read]);
    let root1 = std::env::current_dir().unwrap();
    let label = "current_dir";
    let view1 = FileSystemView::new(root1.clone(), label, permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    assert!(root.change_working_directory(label).unwrap());
    let result = root.change_working_directory("NONEXISTENT");
    let Err(IoError::NotFoundError(_)) = result else {
      panic!("Expected NotFound Error, Got: {:?}", result);
    };
  }

  #[test]
  fn cwd_to_file_system_from_root_absolute_multi_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read]);
    let root1 = std::env::current_dir().unwrap();
    let label = "current_dir";
    let view1 = FileSystemView::new(root1.clone(), label, permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    assert!(root.change_working_directory(&format!("/{label}/test_files")).unwrap());
    assert!(root.current_view.is_some());
    assert_eq!(root.current_view.unwrap(), label);
    assert_eq!(
      root.file_system_views.unwrap().get(label).unwrap().get_display_path(),
      format!("/{label}/test_files")
    );
  }

  #[test]
  fn cwd_from_current_relative_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read]);
    let root1 = std::env::current_dir().unwrap();
    let label = "current_dir";
    let view1 = FileSystemView::new(root1.clone(), label, permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    assert!(root.change_working_directory(&format!("/{label}")).unwrap());
    assert!(root.change_working_directory("test_files").unwrap());
    assert!(root.current_view.is_some());
    assert_eq!(root.current_view.unwrap(), label);
    assert_eq!(
      root.file_system_views.unwrap().get(label).unwrap().get_display_path(),
      format!("/{label}/test_files")
    );
  }

  #[test]
  fn cwd_from_current_to_parent_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read]);
    let root1 = std::env::current_dir().unwrap();
    let label = "current_dir";
    let view1 = FileSystemView::new(root1.clone(), label, permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    assert!(root.change_working_directory(&format!("/{label}")).unwrap());
    assert!(root.change_working_directory("..").unwrap());
    assert!(root.current_view.is_none());
    assert_eq!(
      root.file_system_views.unwrap().get(label).unwrap().get_display_path(),
      format!("/{label}")
    );
  }

  #[test]
  fn create_dir_relative_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Create]);
    let root1 = temp_dir();
    let label = "test";
    let view1 = FileSystemView::new(root1.clone(), label, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));
    let path_start = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = temp_dir().join(&path_start);
    let _cleanup = DirCleanup::new(&dir_path);
    let test_path = format!("{}/{}", label, path_start);

    let result = root.create_directory(&test_path);
    assert!(result.is_ok());
    assert_eq!(format!("/{}", test_path), result.unwrap());
  }

  #[test]
  fn create_dir_relative_multi_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Create]);
    let root1 = temp_dir();
    let label = "test";
    let view1 = FileSystemView::new(root1.clone(), label, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));
    let path_start = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = temp_dir().join(&path_start);
    let _cleanup = DirCleanup::new(&dir_path);
    let test_path = format!("{}/{}/{}", label, path_start, Uuid::new_v4().as_hyphenated());

    let result = root.create_directory(&test_path);
    assert!(result.is_ok());
    assert_eq!(format!("/{}", test_path), result.unwrap());
  }

  #[test]
  fn create_dir_relative_invalid_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Create]);
    let root1 = temp_dir();
    let label = "test";
    let view1 = FileSystemView::new(root1.clone(), label, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));
    let mut invalid_characters = vec!["\0"];

    let mut additional: Vec<String> = Vec::new();
    if cfg!(windows) {
      additional.extend(('\0'..='\u{001F}').map(|c| c.to_string()));
      additional.extend([":", "|", "?", "<", ">", "*"].iter().map(|&a| a.to_string()))
    }
    invalid_characters.extend(additional.iter().map(|c| c.as_str()));
    for path_start in invalid_characters {
      let dir_path = temp_dir().join(path_start);
      let _cleanup = DirCleanup::new(&dir_path);
      let test_path = format!("{}/{}", label, path_start);

      let result = root.create_directory(&test_path);
      let Err(IoError::OsError(_)) = result else {
        panic!("['{}'] Expected InvalidPath Error, Got {:?}", path_start, result);
      };
    }
  }

  #[test]
  fn create_dir_absolute_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Create]);
    let root1 = temp_dir();
    let label = "test";
    let view1 = FileSystemView::new(root1.clone(), label, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));
    let path_start = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = temp_dir().join(&path_start);
    let _cleanup = DirCleanup::new(&dir_path);
    let test_path = format!("/{}/{}", label, path_start);

    let result = root.create_directory(&test_path);
    assert!(result.is_ok());
    assert_eq!(test_path.to_string(), result.unwrap());
  }

  #[test]
  fn create_dir_absolute_multi_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Create]);
    let root1 = temp_dir();
    let label = "test";
    let view1 = FileSystemView::new(root1.clone(), label, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));
    let path_start = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = temp_dir().join(&path_start);
    let _cleanup = DirCleanup::new(&dir_path);
    let test_path = format!("/{}/{}/{}", label, path_start, Uuid::new_v4().as_hyphenated());

    let result = root.create_directory(&test_path);
    assert!(result.is_ok());
    assert_eq!(test_path.to_string(), result.unwrap());
  }

  #[test]
  fn create_dir_then_cwd_unicode_absolute_multi_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Create]);
    let root1 = temp_dir();
    let label = "test";
    let view1 = FileSystemView::new(root1.clone(), label, permissions.clone());
    let views = create_root(vec![view1.clone()]);

    let mut root = FileSystemViewRoot::new(Some(views));
    let path_start = "测试目录";
    let dir_path = temp_dir().join(path_start);
    let _cleanup = DirCleanup::new(&dir_path);
    let test_path = format!("/{}/{}", label, path_start);
    let test_sub_path = "测试子目录";
    let full_test_path = format!("{}/{}", test_path, test_sub_path);

    let result = root.create_directory(&full_test_path);
    assert!(result.is_ok());
    assert_eq!(full_test_path.to_string(), result.unwrap());

    assert!(root.change_working_directory(&test_path).unwrap());
    assert!(root.change_working_directory("..").unwrap());
    assert_eq!(format!("/{label}"), view1.display_path);
    assert_eq!(root1.clone().canonicalize().unwrap(), view1.current_path);
    assert_eq!(root1.clone().canonicalize().unwrap(), view1.root);
  }

  #[test]
  fn list_dir_not_logged_in_empty_test() {
    setup_tracing();
    let root = FileSystemViewRoot::new(None);

    let file = root.list_dir("");
    let Err(IoError::UserError) = file else { panic!("Expected User error") };
  }

  #[test]
  fn list_dir_not_logged_in_relative_test() {
    setup_tracing();
    let root = FileSystemViewRoot::new(None);

    let file = root.list_dir("test_files");
    let Err(IoError::UserError) = file else { panic!("Expected User error") };
  }

  #[test]
  fn list_dir_not_logged_in_absolute_test() {
    setup_tracing();
    let root = FileSystemViewRoot::new(None);

    let file = root.list_dir("/test_files");
    let Err(IoError::UserError) = file else { panic!("Expected User error") };
  }

  #[test]
  fn list_dir_not_logged_in_parent_test() {
    setup_tracing();
    let root = FileSystemViewRoot::new(None);

    let file = root.list_dir("..");
    let Err(IoError::UserError) = file else {
      panic!("Expected User error");
    };
  }

  #[test]
  fn list_dir_root_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);

    let mut root1 = std::env::current_dir().unwrap();
    let mut root2 = std::env::current_dir().unwrap();
    root1.push("src");
    root2.push("test_files");
    let view1 = FileSystemView::new(root1, "src", permissions.clone());
    let view2 = FileSystemView::new(root2, "test_files", permissions.clone());
    let views = create_root(vec![view1, view2]);

    let root = FileSystemViewRoot::new(Some(views));
    let listing = root.list_dir("/").unwrap();
    assert_eq!(3, listing.len());
    assert_eq!(HashSet::<&EntryData>::from_iter(listing.iter()).len(), listing.len());
    validate_listing("/", &listing, 3, 2, 0, 2);
  }

  #[test]
  fn list_dir_root_dot_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);

    let mut root1 = std::env::current_dir().unwrap();
    let mut root2 = std::env::current_dir().unwrap();
    root1.push("src");
    root2.push("test_files");
    let view1 = FileSystemView::new(root1, "src", permissions.clone());
    let view2 = FileSystemView::new(root2, "test_files", permissions.clone());
    let views = create_root(vec![view1, view2]);

    let root = FileSystemViewRoot::new(Some(views));

    let listing = root.list_dir(".").unwrap();
    assert_eq!(3, listing.len());
    assert_eq!(HashSet::<&EntryData>::from_iter(listing.iter()).len(), listing.len());
    validate_listing("/", &listing, 3, 2, 0, 2);
  }

  #[test]
  fn list_dir_root_parent_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);

    let mut root1 = std::env::current_dir().unwrap();
    root1.push("test_files");
    let label = "test_files";
    let view1 = FileSystemView::new(root1, label, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));
    let listing = root.list_dir("..");
    let Err(IoError::InvalidPathError(_)) = listing else {
      panic!("Expected InvalidPath error");
    };
  }

  #[test]
  fn list_dir_current_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);

    let mut root1 = std::env::current_dir().unwrap();
    let mut root2 = std::env::current_dir().unwrap();
    root1.push("src");
    root2.push("test_files");
    let label1 = "src";
    let label2 = "test_files";
    let view1 = FileSystemView::new(root1, label1, permissions.clone());
    let view2 = FileSystemView::new(root2, label2, permissions.clone());
    let views = create_root(vec![view1, view2]);

    let mut root = FileSystemViewRoot::new(Some(views));
    root.change_working_directory("test_files").unwrap();

    let listing = root.list_dir(".").unwrap();
    listing.iter().for_each(|l| print!("{}", l));

    validate_listing("test_files", &listing, 5, permissions.len(), 3, 1);
  }

  #[test]
  fn list_dir_relative_empty_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);

    let mut root1 = std::env::current_dir().unwrap();
    let mut root2 = std::env::current_dir().unwrap();
    root1.push("src");
    root2.push("test_files");
    let label1 = "src";
    let label2 = "test_files";
    let view1 = FileSystemView::new(root1, label1, permissions.clone());
    let view2 = FileSystemView::new(root2, label2, permissions.clone());
    let views = create_root(vec![view1, view2]);

    let mut root = FileSystemViewRoot::new(Some(views));
    root.change_working_directory("test_files").unwrap();

    let listing = root.list_dir("subfolder").unwrap();

    validate_listing("subfolder", &listing, 1, permissions.len(), 0, 0);
  }

  #[test]
  fn list_dir_absolute_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);

    let mut root1 = std::env::current_dir().unwrap();
    let mut root2 = std::env::current_dir().unwrap();
    root1.push("src");
    root2.push("test_files");
    let label1 = "src";
    let label2 = "test_files";
    let view1 = FileSystemView::new(root1, label1, permissions.clone());
    let view2 = FileSystemView::new(root2, label2, permissions.clone());
    let views = create_root(vec![view1, view2]);

    let root = FileSystemViewRoot::new(Some(views));

    let listing = root.list_dir("/test_files").unwrap();

    validate_listing("test_files", &listing, 5, permissions.len(), 3, 1);
  }

  #[test]
  fn list_dir_root_from_view_parent_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);

    let mut root1 = std::env::current_dir().unwrap();
    let mut root2 = std::env::current_dir().unwrap();
    root1.push("src");
    root2.push("test_files");
    let label1 = "src";
    let label2 = "test_files";
    let view1 = FileSystemView::new(root1, label1, permissions.clone());
    let view2 = FileSystemView::new(root2, label2, permissions.clone());
    let views = create_root(vec![view1, view2]);

    let mut root = FileSystemViewRoot::new(Some(views));
    root.change_working_directory(&format!("/{}", label1)).unwrap();

    let listing = root.list_dir("..").map_err(|e| tracing_print!("{}", e)).unwrap();

    assert_eq!(3, listing.len());
    assert_eq!(HashSet::<&EntryData>::from_iter(listing.iter()).len(), listing.len());
    validate_listing("/", &listing, 3, 2, 0, 2);
  }

  #[test]
  fn list_dir_view_parent_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);

    let mut root1 = std::env::current_dir().unwrap();
    root1.push("test_files");
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1, label1, permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    root.change_working_directory(&format!("/{}/subfolder", label1)).unwrap();

    let listing = root.list_dir("..").unwrap();

    validate_listing(label1, &listing, 4, permissions.len(), 3, 1);
  }

  #[tokio::test]
  async fn delete_file_relative_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Delete]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    root.change_working_directory(&format!("/{}", label1)).unwrap();

    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root1.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);

    assert!(file_path.exists());
    let result = root.delete_file(&file_name).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    assert!(!file_path.exists());
  }

  #[tokio::test]
  async fn delete_file_relative_with_label_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Delete]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));

    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root1.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);

    assert!(file_path.exists());
    let result = root.delete_file(&format!("{}/{}", label1, file_name)).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    assert!(!file_path.exists());
  }

  #[tokio::test]
  async fn delete_file_absolute_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Delete]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));

    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root1.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);

    assert!(file_path.exists());
    let result = root.delete_file(&format!("/{}/{}", label1, file_name)).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    assert!(!file_path.exists());
  }

  #[tokio::test]
  async fn delete_file_no_permission_test() {
    setup_tracing();
    let permissions = HashSet::from([]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));

    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root1.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);

    assert!(file_path.exists());
    let result = root.delete_file(&format!("/{}/{}", label1, file_name)).await;
    let Err(IoError::PermissionError) = result else {
      panic!("Expected Permission error, got: {:?}", result);
    };
    assert!(file_path.exists());
  }

  #[tokio::test]
  async fn delete_file_folder_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Delete]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));

    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root1.join(&dir_name);
    create_dir(&dir_path).expect("Test directory should exist");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    let result = root.delete_file(&format!("/{}/{}", label1, dir_name)).await;
    let Err(IoError::NotAFileError) = result else {
      panic!("Expected NotAFile Error, got: {:?}", result);
    };
    assert!(dir_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_relative_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Delete]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    root.change_working_directory(&format!("/{}", label1)).unwrap();

    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root1.join(&dir_name);
    create_dir(&dir_path).expect("Test directory should exist");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    let result = root.delete_folder(&dir_name).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    assert!(!dir_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_relative_with_label_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Delete]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));

    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root1.join(&dir_name);
    create_dir(&dir_path).expect("Test directory should exist");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    let result = root.delete_folder(&format!("{}/{}", label1, dir_name)).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    assert!(!dir_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_absolute_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Delete]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));

    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root1.join(&dir_name);
    create_dir(&dir_path).expect("Test directory should exist");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    let result = root.delete_folder(&format!("/{}/{}", label1, dir_name)).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    assert!(!dir_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_no_permission_test() {
    setup_tracing();
    let permissions = HashSet::from([]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));

    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root1.join(&dir_name);
    create_dir(&dir_path).expect("Test directory should exist");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    let result = root.delete_folder(&format!("/{}/{}", label1, dir_name)).await;
    let Err(IoError::PermissionError) = result else {
      panic!("Expected Permission error, got: {:?}", result);
    };
    assert!(dir_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_file_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Delete]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));

    let file_name = Uuid::new_v4().as_hyphenated().to_string();
    let file_path = root1.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);

    assert!(file_path.exists());
    let result = root.delete_folder(&format!("/{}/{}", label1, file_name)).await;
    let Err(IoError::NotADirectoryError) = result else {
      panic!("Expected NotADirectory Error, got: {:?}", result);
    };
    assert!(file_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_recursive_absolute_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Delete]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    root.change_working_directory(&format!("/{}", label1)).unwrap();

    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root1.join(&dir_name);
    create_dir(&dir_path).expect("Test directory should exist");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    let result = root.delete_folder_recursive(&dir_name).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    assert!(!dir_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_recursive_multi_absolute_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Delete]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));

    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root1.join(&dir_name);
    let dir_sub_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_sub_path = dir_path.join(&dir_sub_name);
    create_dir(&dir_path).expect("Test directory should exist");
    create_dir(&dir_sub_path).expect("Test subdirectory should exist");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    assert!(dir_sub_path.exists());
    let result = root.delete_folder_recursive(&format!("/{}/{}", label1, dir_name)).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    assert!(!dir_path.exists());
    assert!(!dir_sub_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_recursive_relative_with_label_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Delete]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));

    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root1.join(&dir_name);
    create_dir(&dir_path).expect("Test directory should exist");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    let result = root.delete_folder(&format!("{}/{}", label1, dir_name)).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    assert!(!dir_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_recursive_file_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Delete]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));

    let file_name = Uuid::new_v4().as_hyphenated().to_string();
    let file_path = root1.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);

    assert!(file_path.exists());
    let result = root.delete_folder_recursive(&format!("/{}/{}", label1, file_name)).await;
    let Err(IoError::NotADirectoryError) = result else {
      panic!("Expected NotADirectory Error, got: {:?}", result);
    };
    assert!(file_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_recursive_no_permission_test() {
    setup_tracing();
    let permissions = HashSet::from([]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));

    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root1.join(&dir_name);
    create_dir(&dir_path).expect("Test directory should exist");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    let result = root.delete_folder_recursive(&format!("/{}/{}", label1, dir_name)).await;
    let Err(IoError::PermissionError) = result else {
      panic!("Expected Permission error, got: {:?}", result);
    };
    assert!(dir_path.exists());
  }

  #[tokio::test]
  async fn change_file_times_relative_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Write, UserPermission::Execute]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    assert!(root.change_working_directory(&format!("/{}", label1)).unwrap());

    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root1.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);

    assert!(file_path.exists());
    let timeval = Local::now().sub(TimeDelta::hours(4));
    let new_time = FileTimes::new().set_modified(timeval.into());
    let result = root.change_file_times(new_time, &file_name).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    let modification_time: DateTime<Local> =
      File::open(&file_path).unwrap().metadata().unwrap().modified().unwrap().into();
    assert_eq!(timeval, modification_time);
  }

  #[tokio::test]
  async fn change_file_times_relative_with_label_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Write, UserPermission::Execute]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));

    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root1.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);

    assert!(file_path.exists());
    let timeval = Local::now().sub(TimeDelta::hours(4));
    let new_time = FileTimes::new().set_modified(timeval.into());
    let result = root.change_file_times(new_time, &format!("{}/{}", label1, file_name)).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    let modification_time: DateTime<Local> =
      File::open(&file_path).unwrap().metadata().unwrap().modified().unwrap().into();
    assert_eq!(timeval, modification_time);
  }

  #[tokio::test]
  async fn change_file_times_absolute_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Write, UserPermission::Execute]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));

    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root1.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);

    assert!(file_path.exists());
    let timeval = Local::now().sub(TimeDelta::hours(4));
    let new_time = FileTimes::new().set_modified(timeval.into());
    let result = root.change_file_times(new_time, &format!("/{}/{}", label1, file_name)).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    let modification_time: DateTime<Local> =
      File::open(&file_path).unwrap().metadata().unwrap().modified().unwrap().into();
    assert_eq!(timeval, modification_time);
  }

  #[tokio::test]
  async fn change_file_times_no_permission_test() {
    setup_tracing();
    let permissions = HashSet::new();

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));

    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root1.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);

    assert!(file_path.exists());
    let timeval = Local::now().sub(TimeDelta::hours(4));
    let new_time = FileTimes::new().set_modified(timeval.into());
    let result = root.change_file_times(new_time, &format!("/{}/{}", label1, file_name)).await;
    let Err(IoError::PermissionError) = result else {
      panic!("Expected Permission error, got: {:?}", result);
    };
  }

  #[tokio::test]
  async fn change_file_times_folder_test() {
    setup_tracing();
    let permissions = HashSet::from([UserPermission::Write, UserPermission::Execute]);

    let root1 = temp_dir();
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1.clone(), label1, permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));

    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root1.join(&dir_name);
    create_dir(&dir_path).expect("Test directory should exist");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    let timeval = Local::now().sub(TimeDelta::hours(4));
    let new_time = FileTimes::new().set_modified(timeval.into());
    let result = root.change_file_times(new_time, &format!("/{}/{}", label1, dir_name)).await;
    let Err(IoError::NotAFileError) = result else {
      panic!("Expected NotAFile Error, got: {:?}", result);
    };
  }

  #[tokio::test]
  async fn rename_object_same_view_in_view1_relative_relative_test() {
    setup_tracing();
    rename_object_common(true, Some(true), true, true).await;
  }

  #[tokio::test]
  async fn rename_object_same_view_in_view1_relative_absolute_test() {
    setup_tracing();
    rename_object_common(true, Some(true), true, false).await;
  }

  #[tokio::test]
  async fn rename_object_same_view_in_view1_absolute_relative_test() {
    setup_tracing();
    rename_object_common(true, Some(true), false, true).await;
  }

  #[tokio::test]
  async fn rename_object_same_view_in_view1_absolute_absolute_test() {
    setup_tracing();
    rename_object_common(true, Some(true), false, false).await;
  }

  #[tokio::test]
  async fn rename_object_same_view_in_view2_relative_relative_test() {
    setup_tracing();
    rename_object_common(true, Some(false), true, true).await;
  }

  #[tokio::test]
  async fn rename_object_same_view_in_view2_relative_absolute_test() {
    setup_tracing();
    rename_object_common(true, Some(false), true, false).await;
  }

  #[tokio::test]
  async fn rename_object_same_view_in_view2_absolute_relative_test() {
    setup_tracing();
    rename_object_common(true, Some(false), false, true).await;
  }

  #[tokio::test]
  async fn rename_object_same_view_in_view2_absolute_absolute_test() {
    setup_tracing();
    rename_object_common(true, Some(false), false, false).await;
  }

  #[tokio::test]
  async fn rename_object_same_view_in_root_relative_relative_test() {
    setup_tracing();
    rename_object_common(true, None, true, true).await;
  }

  #[tokio::test]
  async fn rename_object_same_view_in_root_relative_absolute_test() {
    setup_tracing();
    rename_object_common(true, None, true, false).await;
  }

  #[tokio::test]
  async fn rename_object_same_view_in_root_absolute_relative_test() {
    setup_tracing();
    rename_object_common(true, None, false, true).await;
  }

  #[tokio::test]
  async fn rename_object_same_view_in_root_absolute_absolute_test() {
    setup_tracing();
    rename_object_common(true, None, false, false).await;
  }

  #[tokio::test]
  async fn rename_object_diff_view_in_view1_relative_absolute_test() {
    setup_tracing();
    rename_object_common(false, Some(true), true, false).await;
  }

  #[tokio::test]
  async fn rename_object_diff_view_in_view1_absolute_absolute_test() {
    setup_tracing();
    rename_object_common(false, Some(true), false, false).await;
  }

  #[tokio::test]
  async fn rename_object_diff_view_in_view2_absolute_absolute_test() {
    setup_tracing();
    rename_object_common(false, Some(false), false, false).await;
  }

  #[tokio::test]
  async fn rename_object_diff_view_in_root_relative_relative_test() {
    setup_tracing();
    rename_object_common(false, None, true, true).await;
  }

  #[tokio::test]
  async fn rename_object_diff_view_in_root_relative_absolute_test() {
    setup_tracing();
    rename_object_common(false, None, true, false).await;
  }

  #[tokio::test]
  async fn rename_object_diff_view_in_root_absolute_relative_test() {
    setup_tracing();
    rename_object_common(false, None, false, true).await;
  }

  #[tokio::test]
  async fn rename_object_diff_view_in_root_absolute_absolute_test() {
    setup_tracing();
    rename_object_common(false, None, false, false).await;
  }

  #[tokio::test]
  async fn rename_object_insufficient_permission_test() {
    setup_tracing();
    let permissions = HashSet::new();

    let label1 = format!("test_files1-{}", Uuid::new_v4().as_hyphenated());
    let view1 = FileSystemView::new(temp_dir(), &label1, permissions.clone());
    let views = create_root(vec![view1]);
    let root = FileSystemViewRoot::new(Some(views));

    let file_name1 = format!("{}/{}_1.test", label1, Uuid::new_v4().as_hyphenated());
    let file_name2 = format!("{}/{}_2.test", label1, Uuid::new_v4().as_hyphenated());
    let Err(IoError::PermissionError) = root.rename_object(&file_name1, &file_name2).await else {
      panic!("Expected permission error");
    };
  }

  #[tokio::test]
  async fn rename_object_nonexistent_file_test() {
    setup_tracing();
    let permissions =
      HashSet::from([UserPermission::Read, UserPermission::Write, UserPermission::Delete]);

    let label1 = format!("test_files1-{}", Uuid::new_v4().as_hyphenated());
    let view1 = FileSystemView::new(temp_dir(), &label1, permissions.clone());
    let views = create_root(vec![view1]);
    let root = FileSystemViewRoot::new(Some(views));

    let file_name1 = format!("{}/{}_1.test", label1, Uuid::new_v4().as_hyphenated());
    let file_name2 = format!("{}/{}_2.test", label1, Uuid::new_v4().as_hyphenated());
    let result = root.rename_object(&file_name1, &file_name2).await;
    tracing_error!("Result: {result:?}");
    let Err(IoError::NotFoundError(_)) = result else {
      panic!("Expected not found error");
    };
  }

  #[tokio::test]
  async fn rename_object_root_test() {
    setup_tracing();
    let permissions =
      HashSet::from([UserPermission::Read, UserPermission::Write, UserPermission::Delete]);

    let label1 = format!("test_files1-{}", Uuid::new_v4().as_hyphenated());
    let file_name1 = format!("test_file1-{}", Uuid::new_v4().as_hyphenated());
    let file_path1 = temp_dir().join(&file_name1);
    let view1 = FileSystemView::new(temp_dir(), &label1, permissions.clone());
    let views = create_root(vec![view1]);
    let root = FileSystemViewRoot::new(Some(views));

    touch(&file_path1).unwrap();
    let args = vec![
      ("", file_name1.as_str()),
      (file_name1.as_str(), ""),
      ("", ""),
      ("/", file_name1.as_str()),
      (file_name1.as_str(), "/"),
      ("/", "/"),
    ];
    for (file_name1, file_name2) in args {
      let result = root.rename_object(file_name1, file_name2).await;
      info!("Result: {result:?}");
      let Err(IoError::InvalidPathError(_)) = result else {
        panic!("Expected invalid path error");
      };
    }
  }

  /// in_view: Some(true) = in view1, Some(false) = in view2, None = in neither view
  async fn rename_object_common(
    same_view: bool,
    in_view: Option<bool>,
    path1_relative: bool,
    path2_relative: bool,
  ) {
    setup_tracing();
    let permissions =
      HashSet::from([UserPermission::Read, UserPermission::Write, UserPermission::Delete]);

    let temp_dir = temp_dir();

    let label1 = format!("test_files1-{}", Uuid::new_v4().as_hyphenated());
    let view_path1 = temp_dir.join(&label1);
    create_dir(&view_path1).expect("Test directory 1 should exist");
    let _cleanup_dir1 = DirCleanup::new(&view_path1);
    let view1 = FileSystemView::new(temp_dir.join(&label1), &label1, permissions.clone());

    let label2 = format!("test_files2-{}", Uuid::new_v4().as_hyphenated());
    let view_path2 = temp_dir.join(&label2);
    create_dir(&view_path2).expect("Test directory 2 should exist");
    let _cleanup_dir2 = DirCleanup::new(&view_path2);
    let view2 = FileSystemView::new(temp_dir.join(&label2), &label2, permissions.clone());
    let views = create_root(vec![view1, view2]);

    let mut root = FileSystemViewRoot::new(Some(views));
    if let Some(iv) = in_view {
      if iv {
        assert!(root.change_working_directory(&format!("/{}", label1)).unwrap());
      } else {
        assert!(root.change_working_directory(&format!("/{}", label2)).unwrap());
      }
    }

    info!("Current path: {:?}", root.get_current_working_directory());
    let file_name1 = format!("{}_1.test", Uuid::new_v4().as_hyphenated());
    let file_path1 = match (in_view, same_view) {
      (Some(true), true) | (None, true) | (_, false) => view_path1.join(&file_name1),
      (Some(false), true) => view_path2.join(&file_name1),
    };
    generate_test_file(2u64.pow(20) as usize, &file_path1).await;
    let _cleanup1 = FileCleanup::new(&file_path1);

    let mut hasher1 = Hasher::new();
    hasher1.update_reader(File::open(&file_path1).unwrap()).unwrap();
    let hash1 = hasher1.finalize();

    let file_name2 = format!("{}_2.test", Uuid::new_v4().as_hyphenated());
    let file_path2 = match (same_view, in_view) {
      (true, Some(true)) | (true, None) => view_path1.join(&file_name2),
      (true, Some(false)) | (false, _) => view_path2.join(&file_name2),
    };
    let _cleanup2 = same_view.then(|| FileCleanup::new(&file_path2));

    let from_path = match (same_view, in_view, path1_relative) {
      (true, Some(_), true) | (false, Some(true), true) => file_name1.to_string(),
      (true, None, true) | (false, None, true) => format!("{}/{}", label1, file_name1),
      (true, Some(false), false) => format!("/{}/{}", label2, file_name1),
      (true, Some(_), false) | (true, _, false) | (_, _, false) => {
        format!("/{}/{}", label1, file_name1)
      }
      (false, Some(false), true) => panic!("Invalid test parameters"),
    };

    let to_path = match (same_view, in_view, path2_relative) {
      (true, Some(_), true) | (false, Some(false), true) => file_name2.to_string(),
      (true, None, true) => format!("{}/{}", label1, file_name2),
      (false, None, true) => format!("{}/{}", label2, file_name2),
      (true, Some(true), false) | (true, None, false) => format!("/{}/{}", label1, file_name2),
      (false, Some(true), true) => panic!("Invalid test parameters"),
      (_, _, false) => format!("/{}/{}", label2, file_name2),
    };

    assert!(!file_path2.exists());
    root.rename_object(&from_path, &to_path).await.unwrap();
    assert!(!file_path1.exists());
    info!("file_path2 = {:?}", &file_path2);

    let mut hasher2 = Hasher::new();
    hasher2.update_reader(File::open(&file_path2).unwrap()).unwrap();
    let hash2 = hasher2.finalize();

    assert_eq!(hash1, hash2);
  }

  pub(crate) fn create_root(views: Vec<FileSystemView>) -> HashMap<String, ViewDispatch> {
    views.into_iter().map(|v| (v.label.clone(), v.into())).collect()
  }
}
