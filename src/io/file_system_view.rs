//! File system view represent an abstraction over the file system. Each view corresponds to a
//! single location a user can access. This can be a disk partition or a specific directory.
//! The user has a set of permissions which specify which operations are permitted.

use async_trait::async_trait;
use path_clean::PathClean;
use std::collections::HashSet;
use std::fs::{FileTimes, ReadDir, create_dir_all};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tracing::debug;
use unicode_segmentation::UnicodeSegmentation;

use crate::auth::user_permission::UserPermission;
use crate::io::entry_data::{EntryData, EntryType};
use crate::io::error::IoError;
use crate::io::open_options_flags::OpenOptionsWrapperBuilder;
use crate::io::view::View;

/// For documentation about file system view, see [`module`] documentation.
///
/// [`module`]: crate::io::file_system_view
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileSystemView {
  pub(crate) root: PathBuf,         // native path to starting directory
  pub(crate) current_path: PathBuf, // native path to current directory
  pub(crate) display_path: String,  // virtual path
  pub(crate) label: String,
  pub(crate) permissions: HashSet<UserPermission>,
}

impl FileSystemView {
  /// Creates a new instance of a [`FileSystemView`].
  ///
  /// This function takes in a `root` path, a `label`, and a set of `permissions`, and returns a
  /// new `FileSystemView` instance.
  ///
  /// # Arguments
  /// - `root`: A [`PathBuf`] representing the root path of the file system view.
  /// - `label`: A type that can be converted into a [`String`], representing the label of the file
  ///   system view.
  /// - `permissions`: A [`HashSet<UserPermission>`] containing the set of permissions the user has
  ///   in the view.
  ///
  /// # Panics
  ///
  /// This function will panic if the root path cannot be canonicalized, i.e., if it does not
  /// exist.
  ///
  /// # Returns
  ///
  /// New [`FileSystemView`] instance.
  ///
  #[cfg(test)]
  pub(crate) fn new(root: PathBuf, label: &str, permissions: HashSet<UserPermission>) -> Self {
    let label = label.into();
    let root = root.canonicalize().expect("View path must exist!");
    FileSystemView {
      current_path: root.clone(),
      root,
      display_path: format!("/{}", label),
      label,
      permissions,
    }
  }

  /// Creates a new instance of a `FileSystemView`.
  ///
  /// This function takes in a `root` path, a `label`, and a set of `permissions`, and returns a
  /// [`Ok<FileSystemView>`]. If the root path cannot be canonicalized, then this will
  /// return [`Err(())`].
  ///
  /// # Arguments
  ///
  /// - `root`: A [`PathBuf`] representing the root path of the file system view.
  /// - `label`: A type that can be converted into a [`String`], representing the label of the
  ///   file system view.
  /// - `permissions`: A [`HashSet<UserPermission>`] containing the set of permissions the user has
  ///   in the view.
  ///
  /// # Returns
  ///
  /// A [`Result`] containing the new [`FileSystemView`] if successful, or [`Err`] if an error
  /// occurs.
  ///
  pub(crate) fn new_option(
    root: PathBuf,
    label: &str,
    permissions: HashSet<UserPermission>,
  ) -> Result<Self, ()> {
    let label = label.into();
    match root.canonicalize() {
      Ok(r) => Ok(FileSystemView {
        current_path: r.clone(),
        root: r,
        display_path: format!("/{}", label),
        label,
        permissions,
      }),
      Err(_) => Err(()),
    }
  }

  /// Convert the listing of objects in directory to common format.
  ///
  /// This function converts a raw [`ReadDir`] into a [`Vec`] of [`EntryData`] and then returns it.
  ///
  /// # Arguments
  /// - `name`: A type that can be converted into a [`String`], representing the name of the
  ///   listed directory.
  /// - `path`: A type that can be converted into a [`String`], representing the path to the
  ///   listed directory.
  /// - `read_dir`: A [`ReadDir`] containing all the listed objects.
  /// - `permissions`: A [`HashSet<UserPermission>`] containing the set of permissions the user has
  ///   for the objects.
  ///
  /// # Returns
  ///
  /// A [`Vec<EntryData>`] containing the converted listing.
  ///
  fn create_listing(
    name: &str,
    path: impl AsRef<Path>,
    read_dir: ReadDir,
    permissions: &HashSet<UserPermission>,
  ) -> Vec<EntryData> {
    let mut listing = Vec::with_capacity(read_dir.size_hint().0 + 1);
    if let Ok(meta) = path.as_ref().metadata() {
      let mut cdir = EntryData::create_from_metadata(meta, name, permissions);
      cdir.change_entry_type(EntryType::Cdir);
      listing.push(cdir);
    }

    let mut entries: Vec<EntryData> = read_dir
      .filter_map(|d| d.ok())
      .filter_map(|e| {
        let name = e.file_name().into_string().unwrap();
        if let Ok(meta) = e.metadata() {
          return Some(EntryData::create_from_metadata(meta, name, permissions));
        }
        None
      })
      .collect();

    listing.append(&mut entries);
    listing
  }
}

#[async_trait]
impl View for FileSystemView {
  /// Changes the current path to the specified one.
  ///
  /// This function changes the current path to `path` and returns [`Ok`] if the new path is valid,
  /// [`Err(IoError)`] otherwise. New path can be absolute or relative and also the current path
  /// (.), parent (..) and root (/).
  ///
  /// # Arguments
  ///
  /// `path`: A type that can be converted into a [`String`], that will be used to construct the
  /// new path.
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
  /// - [`IoError::NotFoundError`]: If the new path does not exist.
  /// - [`IoError::PermissionError`]: If the user does not have access permissions.
  /// - [`IoError::OsError`]: If the OS reports any other error.
  /// - [`IoError::NotADirectoryError`]: If the `path` does not refer to a directory.
  /// - [`IoError::InvalidPathError`]: If the `path` refers to parent (..) but current path is
  ///   already at root.
  ///
  fn change_working_directory(&mut self, path: &str) -> Result<bool, IoError> {
    let path = path.replace('\\', "/");
    let current_path = self.current_path.clone();
    if path.is_empty() || path == "." {
      return Ok(false);
    } else if path == ".." {
      if self.current_path == self.root {
        return Err(IoError::InvalidPathError(String::from("Cannot change to parent from root!")));
      }
      self.current_path.pop();
      if self.display_path != "/" {
        // display_path.rfind("/") does not work when display_path contains values spanning
        // multiple unicode scalar points
        let new_display_path: String = self
          .display_path
          .graphemes(true)
          .rev()
          .skip_while(|&c| c != "/")
          .skip(1)
          .collect::<String>()
          .graphemes(true)
          .rev()
          .collect();

        self.display_path = new_display_path;
        if self.display_path.is_empty() {
          self.display_path = format!("/{}", self.label.clone());
        }
      }
    } else if path == "~" || path == "/" {
      self.display_path = format!("/{}", self.label);
      self.current_path.clone_from(&self.root);
    } else if let Some(stripped) = path.strip_prefix('/') {
      let new_current = match self.root.join(stripped).canonicalize() {
        Ok(n) => n,
        Err(e) => return Err(IoError::map_io_error(e)),
      };

      self.current_path = new_current;
      self.display_path = format!("/{}{}", &self.label, &path);
    } else {
      let new_current = match self.current_path.join(path.clone()).canonicalize() {
        Ok(n) => {
          if !n.is_dir() {
            return Err(IoError::NotADirectoryError);
          }
          n
        }
        Err(e) => return Err(IoError::map_io_error(e)),
      };

      self.current_path = new_current;
      self.display_path.push('/');
      self.display_path.push_str(&path);
    }
    Ok(self.current_path != current_path)
  }
  fn create_directory(&self, path: &str) -> Result<String, IoError> {
    let mut path = path.replace('\\', "/");

    if !self.permissions.contains(&UserPermission::Create) {
      return Err(IoError::PermissionError);
    }
    let mut virtual_path = self.display_path.clone();

    let new_directory_path = self.process_path(&path).clean();

    if !new_directory_path.starts_with(&self.root) {
      return Err(IoError::InvalidPathError(String::from("Invalid path!")));
    }

    if !path.starts_with('/') {
      virtual_path.push('/');
    }

    if let Some(stripped) = path.strip_prefix(&format!("/{}", &self.label)) {
      path = stripped.to_string();
    }

    virtual_path.push_str(&path);

    create_dir_all(&new_directory_path).map(|_| virtual_path).map_err(IoError::map_io_error)
  }

  async fn delete_file(&self, path: &str) -> Result<(), IoError> {
    if !self.permissions.contains(&UserPermission::Delete) {
      return Err(IoError::PermissionError);
    }

    let path = self.process_path(path);
    let path = if !path.exists() {
      return Err(IoError::NotFoundError("File not found".to_string()));
    } else if !path.is_file() {
      return Err(IoError::NotAFileError);
    } else {
      path.canonicalize().map_err(IoError::map_io_error)?
    };

    if !path.starts_with(&self.root) {
      return Err(IoError::InvalidPathError(String::from("Invalid path!")));
    }

    debug!("Deleting: {:?}", &path);

    match tokio::fs::remove_file(path).await {
      Ok(()) => Ok(()),
      Err(e) => Err(IoError::map_io_error(e)),
    }
  }

  async fn delete_folder(&self, path: &str) -> Result<(), IoError> {
    if !self.permissions.contains(&UserPermission::Delete) {
      return Err(IoError::PermissionError);
    }

    let path = self.process_path(path);
    let path = if !path.exists() {
      return Err(IoError::NotFoundError("Directory not found".to_string()));
    } else if !path.is_dir() {
      return Err(IoError::NotADirectoryError);
    } else {
      path.canonicalize().map_err(IoError::map_io_error)?
    };

    if !path.starts_with(&self.root) {
      return Err(IoError::InvalidPathError(String::from("Invalid path!")));
    }

    debug!("Deleting: {:?}", &path);

    match tokio::fs::remove_dir(path).await {
      Ok(()) => Ok(()),
      Err(e) => Err(IoError::map_io_error(e)),
    }
  }

  async fn delete_folder_recursive(&self, path: &str) -> Result<(), IoError> {
    if !self.permissions.contains(&UserPermission::Delete) {
      return Err(IoError::PermissionError);
    }

    let path = self.process_path(path);
    let path = if !path.exists() {
      return Err(IoError::NotFoundError("Directory not found".to_string()));
    } else if !path.is_dir() {
      return Err(IoError::NotADirectoryError);
    } else {
      path.canonicalize().map_err(IoError::map_io_error)?
    };

    if !path.starts_with(&self.root) {
      return Err(IoError::InvalidPathError(String::from("Invalid path!")));
    }

    debug!("Deleting: {:?}", &path);

    match tokio::fs::remove_dir_all(path).await {
      Ok(()) => Ok(()),
      Err(e) => Err(IoError::map_io_error(e)),
    }
  }

  async fn change_file_times(&self, new_time: FileTimes, path: &str) -> Result<(), IoError> {
    if !self.permissions.contains(&UserPermission::Execute)
      || !self.permissions.contains(&UserPermission::Write)
    {
      return Err(IoError::PermissionError);
    }

    self
      .open_file(path, OpenOptionsWrapperBuilder::default().write(true).build().unwrap())
      .await?
      .into_std()
      .await
      .set_times(new_time)
      .map_err(IoError::map_io_error)
  }

  fn list_dir(&self, path: &str) -> Result<Vec<EntryData>, IoError> {
    if !self.permissions.contains(&UserPermission::List) {
      return Err(IoError::PermissionError);
    }

    if path.is_empty() || path == "." {
      // List current dir
      let current = &self.current_path;
      if !current.exists() {
        // Path doesn't exist! Nothing to list
        panic!("Current path should always exist!");
      }

      let read_dir = match current.read_dir() {
        Ok(read_dir) => read_dir,
        Err(e) => return Err(IoError::OsError(e)), // IO Error
      };

      let name = self.display_path.rsplit_once('/').unwrap_or(("", &self.label)).1;

      Ok(Self::create_listing(name, current, read_dir, &self.permissions))
    } else if path == ".." {
      if self.root == self.current_path {
        // Cannot list before root
        // MUST RETURN InvalidPathError ONLY HERE
        return Err(IoError::InvalidPathError(String::new()));
      }

      let parent = self.current_path.parent();
      if parent.is_none() {
        // Path doesn't exist! Nothing to list
        panic!("Parent path should always exist, as long as root != current_path!");
      }
      let parent = parent.unwrap();

      let read_dir = match parent.read_dir() {
        Ok(read_dir) => read_dir,
        Err(e) => return Err(IoError::OsError(e)), // IO Error
      };

      let parent_name = parent.file_name().map(|n| n.to_str().unwrap()).unwrap_or("");

      Ok(Self::create_listing(parent_name, parent, read_dir, &self.permissions))
    } else if path == "/" || path == "~" {
      // List root
      if !self.root.exists() {
        // Root doesn't exist! Should we panic?
        panic!("View root should always exist!");
      }

      let read_dir = match self.root.read_dir() {
        Ok(read_dir) => read_dir,
        Err(e) => return Err(IoError::OsError(e)), // IO Error
      };

      Ok(Self::create_listing(&self.label, self.root.clone(), read_dir, &self.permissions))
    } else if let Some(stripped) = path.strip_prefix('/') {
      let absolute = match self.root.join(stripped).canonicalize() {
        Ok(absolute) => absolute,
        // Path doesn't exist! Nothing to list
        Err(e) => {
          return match e.kind() {
            ErrorKind::NotFound => {
              Err(IoError::NotFoundError(String::from("Directory not found!")))
            }
            // Path does not refer to a directory
            _ => Err(IoError::NotADirectoryError),
          };
        }
      };

      let read_dir = match absolute.read_dir() {
        Ok(read_dir) => read_dir,
        Err(e) => return Err(IoError::OsError(e)), // IO Error
      };

      Ok(Self::create_listing(
        path.rsplit_once('/').unwrap().1,
        absolute,
        read_dir,
        &self.permissions,
      ))
    } else {
      let relative = self.current_path.join(path);
      if !relative.exists() {
        // Path doesn't exist! Nothing to list
        return Err(IoError::NotFoundError(String::from("Directory not found!")));
      }

      if !relative.is_dir() {
        // Path does not refer to a directory
        return Err(IoError::NotADirectoryError);
      }

      let read_dir = match relative.read_dir() {
        Ok(read_dir) => read_dir,
        Err(e) => return Err(IoError::OsError(e)), // IO Error
      };

      Ok(Self::create_listing(
        path.rsplit_once('/').unwrap_or(("", path)).1,
        relative,
        read_dir,
        &self.permissions,
      ))
    }
  }

  fn get_label(&self) -> &str {
    &self.label
  }

  fn get_display_path(&self) -> &str {
    &self.display_path
  }

  fn get_permissions(&self) -> &HashSet<UserPermission> {
    &self.permissions
  }

  fn get_current_path(&self) -> &Path {
    &self.current_path
  }

  fn get_root_path(&self) -> &Path {
    &self.root
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use std::collections::HashSet;
  use std::env::{current_dir, temp_dir};
  use std::fs::{File, FileTimes};
  use std::ops::Sub;

  use chrono::{DateTime, Local, TimeDelta};
  use uuid::Uuid;

  use crate::auth::user_permission::UserPermission;
  use crate::io::entry_data::{EntryData, EntryType};
  use crate::io::error::IoError;
  use crate::io::file_system_view::FileSystemView;
  use crate::io::open_options_flags::OpenOptionsWrapperBuilder;
  use crate::io::view::View;
  use crate::utils::test_utils::*;

  #[test]
  fn derives_test() {
    let permissions = HashSet::from([UserPermission::Read]);
    let root = current_dir().unwrap();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);

    assert_eq!(view.clone(), view);
    assert_eq!(view, view);
  }

  #[test]
  fn cwd_to_sub_test() {
    let permissions = HashSet::from([UserPermission::Read]);
    let root = current_dir().unwrap();
    let label = "test";
    let mut view = FileSystemView::new(root.clone(), label, permissions);

    assert!(view.change_working_directory("test_files").unwrap());
    assert_eq!(format!("/{label}/test_files"), view.display_path);
    assert_eq!(root.join("test_files").canonicalize().unwrap(), view.current_path);
    assert_eq!(root.canonicalize().unwrap(), view.root);
  }

  #[test]
  fn cwd_to_sub_nonexistent_test() {
    let permissions = HashSet::from([UserPermission::Read]);
    let root = current_dir().unwrap();
    let label = "test";
    let mut view = FileSystemView::new(root.clone(), label, permissions);

    let change = view.change_working_directory("NONEXISTENT");
    let Err(IoError::NotFoundError(_)) = change else {
      panic!("Expected NotFound error, got: {:?}", change);
    };
    assert_eq!(format!("/{label}"), view.display_path);
    assert_eq!(root.clone().canonicalize().unwrap(), view.current_path);
    assert_eq!(root.canonicalize().unwrap(), view.root);
  }

  #[test]
  fn cwd_to_absolute_nonexistent_test() {
    let permissions = HashSet::from([UserPermission::Read]);
    let root = current_dir().unwrap();
    let label = "test";
    let mut view = FileSystemView::new(root.clone(), label, permissions);

    let change = view.change_working_directory("/NONEXISTENT");
    let Err(IoError::NotFoundError(_)) = change else {
      panic!("Expected NotFound error, got: {:?}", change);
    };
    assert_eq!(format!("/{label}"), view.display_path);
    assert_eq!(root.clone().canonicalize().unwrap(), view.current_path);
    assert_eq!(root.canonicalize().unwrap(), view.root);
  }

  #[test]
  fn cwd_to_absolute_test() {
    let permissions = HashSet::from([UserPermission::Read]);
    let root = current_dir().unwrap();
    let label = "test";
    let mut view = FileSystemView::new(root.clone(), label, permissions);

    assert!(view.change_working_directory("/test_files").unwrap());
    assert_eq!(format!("/{label}/test_files"), view.display_path);
    assert_eq!(root.join("test_files").canonicalize().unwrap(), view.current_path);
    assert_eq!(root.canonicalize().unwrap(), view.root);
  }

  #[test]
  fn cwd_to_absolute_multi_test() {
    let permissions = HashSet::from([UserPermission::Read]);
    let root = current_dir().unwrap();
    let label = "test";
    let mut view = FileSystemView::new(root.clone(), label, permissions);

    assert!(view.change_working_directory("/test_files/subfolder").unwrap());
    assert_eq!(format!("/{label}/test_files/subfolder"), view.display_path);
    assert_eq!(root.join("test_files/subfolder").canonicalize().unwrap(), view.current_path);
    assert_eq!(root.canonicalize().unwrap(), view.root);
  }

  #[test]
  fn cwd_to_dot_test() {
    let permissions = HashSet::from([UserPermission::Read]);
    let root = current_dir().unwrap();
    let label = "test";
    let mut view = FileSystemView::new(root.clone(), label, permissions);

    assert!(!view.change_working_directory(".").unwrap());
    assert_eq!(format!("/{label}"), view.display_path);
    assert_eq!(root.clone().canonicalize().unwrap(), view.current_path);
    assert_eq!(root.canonicalize().unwrap(), view.root);
  }

  #[test]
  fn cwd_to_parent_test() {
    let permissions = HashSet::from([UserPermission::Read]);
    let root = current_dir().unwrap();
    let label = "test";
    let mut view = FileSystemView::new(root.clone(), label, permissions);

    assert!(view.change_working_directory("test_files").unwrap());
    assert!(view.change_working_directory("..").unwrap());
    assert_eq!(format!("/{label}"), view.display_path);
    assert_eq!(root.clone().canonicalize().unwrap(), view.current_path);
    assert_eq!(root.canonicalize().unwrap(), view.root);
  }

  #[test]
  fn cwd_to_parent_from_root_test() {
    let permissions = HashSet::from([UserPermission::Read]);
    let root = current_dir().unwrap().join("test_files");
    let label = "test";
    let mut view = FileSystemView::new(root.clone(), label, permissions);

    let change = view.change_working_directory("..");
    let Err(IoError::InvalidPathError(_)) = change else {
      panic!("Expected InvalidPath error, got: {:#?}", change);
    };
    assert_eq!(format!("/{label}"), view.display_path);
    assert_eq!(root.clone().canonicalize().unwrap(), view.current_path);
    assert_eq!(root.canonicalize().unwrap(), view.root);
  }

  #[test]
  fn cwd_to_home_test() {
    let permissions = HashSet::from([UserPermission::Read]);
    let root = current_dir().unwrap();
    let label = "test";
    let mut view = FileSystemView::new(root.clone(), label, permissions);

    assert!(view.change_working_directory("test_files").unwrap());
    assert!(view.change_working_directory("subfolder").unwrap());
    assert!(view.change_working_directory("~").unwrap());
    assert_eq!(format!("/{label}"), view.display_path);
    assert_eq!(root.clone().canonicalize().unwrap(), view.current_path);
    assert_eq!(root.canonicalize().unwrap(), view.root);
  }

  #[tokio::test]
  async fn open_file_relative_test() {
    let permissions = HashSet::from([UserPermission::Read]);
    let root = current_dir().unwrap();
    let label = "test";
    let mut view = FileSystemView::new(root.clone(), label, permissions);

    assert!(view.change_working_directory("test_files").unwrap());
    let options = OpenOptionsWrapperBuilder::default().read(true).build().unwrap();
    let file_path = view.open_file("1MiB.txt", options).await;
    assert!(file_path.is_ok());
  }

  #[tokio::test]
  async fn open_file_relative_multi_test() {
    let permissions = HashSet::from([UserPermission::Read]);
    let root = current_dir().unwrap();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);

    let options = OpenOptionsWrapperBuilder::default().read(true).build().unwrap();
    let file_path = view.open_file("test_files/1MiB.txt", options).await;
    assert!(file_path.is_ok());
  }

  #[tokio::test]
  async fn open_file_absolute_test() {
    let permissions = HashSet::from([UserPermission::Read]);
    let root = current_dir().unwrap();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);

    let options = OpenOptionsWrapperBuilder::default().read(true).build().unwrap();
    let file = view.open_file("/test_files/1MiB.txt", options).await;
    assert!(file.is_ok());
  }

  #[tokio::test]
  async fn open_file_no_permissions_test() {
    let root = current_dir().unwrap();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, HashSet::new());

    let options = OpenOptionsWrapperBuilder::default().read(true).build().unwrap();
    let file = view.open_file("/test_files/1MiB.txt", options).await;
    let Err(IoError::PermissionError) = file else {
      panic!("Expected Permission error, got: {:?}", file);
    };
  }

  #[tokio::test]
  async fn open_file_directory_test() {
    let permissions = HashSet::from([UserPermission::Read]);
    let root = current_dir().unwrap();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());

    let options = OpenOptionsWrapperBuilder::default().read(true).build().unwrap();
    let file = view.open_file("/test_files/subfolder", options).await;
    let Err(IoError::NotAFileError) = file else {
      panic!("Expected NotAFile error, got: {:?}", file);
    };
  }

  #[tokio::test]
  async fn delete_file_absolute_test() {
    let permissions = HashSet::from([UserPermission::Delete]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);

    assert!(file_path.exists());
    let result = view.delete_file(&format!("/{label}/{file_name}")).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    assert!(!file_path.exists());
  }

  #[tokio::test]
  async fn delete_file_relative_test() {
    let permissions = HashSet::from([UserPermission::Delete]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);

    assert!(file_path.exists());
    let result = view.delete_file(&file_name).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    assert!(!file_path.exists());
  }

  #[tokio::test]
  async fn delete_file_directory_test() {
    let permissions = HashSet::from([UserPermission::Delete]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);
    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root.join(&dir_name);
    create_dir(&dir_path).expect("Test directory should exist");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    let result = view.delete_file(&dir_name).await;

    let Err(IoError::NotAFileError) = result else {
      panic!("Expected NotAFile Error, got: {:?}", result);
    };
    assert!(dir_path.exists());
  }

  #[tokio::test]
  async fn delete_file_no_permissions_test() {
    let permissions = HashSet::from([]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);

    assert!(file_path.exists());
    let result = view.delete_file(&file_name).await;
    let Err(IoError::PermissionError) = result else {
      panic!("Expected Permission Error, got: {:?}", result);
    };
    assert!(file_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_absolute_test() {
    let permissions = HashSet::from([UserPermission::Delete]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);
    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root.join(&dir_name);
    create_dir(&dir_path).expect("Test directory should exist");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    let result = view.delete_folder(&format!("/{dir_name}")).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    assert!(!dir_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_relative_test() {
    let permissions = HashSet::from([UserPermission::Delete]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);
    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root.join(&dir_name);
    create_dir(&dir_path).expect("Test directory should exist");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    let result = view.delete_folder(&dir_name).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    assert!(!dir_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_file_test() {
    let permissions = HashSet::from([UserPermission::Delete]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);

    assert!(file_path.exists());
    let result = view.delete_folder(&file_name).await;
    let Err(IoError::NotADirectoryError) = result else {
      panic!("Expected NotADirectory Error, got: {:?}", result);
    };
    assert!(file_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_no_permissions_test() {
    let permissions = HashSet::from([]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);
    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root.join(&dir_name);
    create_dir(&dir_path).expect("Test directory should exist");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    let result = view.delete_folder(&dir_name).await;
    let Err(IoError::PermissionError) = result else {
      panic!("Expected Permission Error, got: {:?}", result);
    };
    assert!(dir_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_recursive_absolute_test() {
    let permissions = HashSet::from([UserPermission::Delete]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);
    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root.join(&dir_name);
    create_dir(&dir_path).expect("Test directory should exist");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    let result = view.delete_folder_recursive(&format!("/{dir_name}")).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    assert!(!dir_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_recursive_multi_absolute_test() {
    let permissions = HashSet::from([UserPermission::Delete]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);
    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root.join(&dir_name);
    let dir_sub_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_sub_path = dir_path.join(&dir_sub_name);
    create_dir(&dir_path).expect("Test directory should exist");
    std::fs::create_dir(&dir_sub_path).expect("Creating test directory should succeed");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    assert!(dir_sub_path.exists());
    let result = view.delete_folder_recursive(&format!("/{dir_name}")).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    assert!(!dir_path.exists());
    assert!(!dir_sub_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_recursive_relative_test() {
    let permissions = HashSet::from([UserPermission::Delete]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);
    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root.join(&dir_name);
    create_dir(&dir_path).expect("Test directory should exist");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    let result = view.delete_folder_recursive(&dir_name).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    assert!(!dir_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_recursive_multi_relative_test() {
    let permissions = HashSet::from([UserPermission::Delete]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);
    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root.join(&dir_name);
    let dir_sub_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_sub_path = dir_path.join(&dir_sub_name);
    create_dir(&dir_path).expect("Test directory should exist");
    std::fs::create_dir(&dir_sub_path).expect("Creating test directory should succeed");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    assert!(dir_sub_path.exists());
    let result = view.delete_folder_recursive(&dir_name).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    assert!(!dir_path.exists());
    assert!(!dir_sub_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_recursive_file_test() {
    let permissions = HashSet::from([UserPermission::Delete]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);
    let file_name = Uuid::new_v4().as_hyphenated().to_string();
    let file_path = root.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);

    assert!(file_path.exists());
    let result = view.delete_folder_recursive(&file_name).await;
    let Err(IoError::NotADirectoryError) = result else {
      panic!("Expected NotADirectory Error, got: {:?}", result);
    };
    assert!(file_path.exists());
  }

  #[tokio::test]
  async fn delete_folder_recursive_no_permissions_test() {
    let permissions = HashSet::from([]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);
    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root.join(&dir_name);
    let dir_sub_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_sub_path = dir_path.join(&dir_sub_name);
    create_dir(&dir_path).expect("Test directory should exist");
    std::fs::create_dir(&dir_sub_path).expect("Creating test directory should succeed");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    let result = view.delete_folder_recursive(&dir_name).await;
    let Err(IoError::PermissionError) = result else {
      panic!("Expected Permission Error, got: {:?}", result);
    };
    assert!(dir_path.exists());
  }

  #[tokio::test]
  async fn change_file_times_absolute_test() {
    let permissions = HashSet::from([UserPermission::Write, UserPermission::Execute]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);

    assert!(file_path.exists());
    let timeval = Local::now().sub(TimeDelta::hours(4));
    let new_time = FileTimes::new().set_modified(timeval.into());
    let result = view.change_file_times(new_time, &format!("/{label}/{file_name}")).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    let modification_time: DateTime<Local> =
      File::open(&file_path).unwrap().metadata().unwrap().modified().unwrap().into();
    assert_eq!(timeval, modification_time);
  }

  #[tokio::test]
  async fn change_file_times_relative_test() {
    let permissions = HashSet::from([UserPermission::Write, UserPermission::Execute]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);
    let file_name = format!("{}.test", Uuid::new_v4().as_hyphenated());
    let file_path = root.join(&file_name);
    touch(&file_path).expect("Test file must exist");

    let _cleanup = FileCleanup::new(&file_path);

    assert!(file_path.exists());
    let timeval = Local::now().sub(TimeDelta::hours(4));
    let new_time = FileTimes::new().set_modified(timeval.into());
    let result = view.change_file_times(new_time, &file_name).await;
    let Ok(()) = result else {
      panic!("Expected OK, got: {:?}", result);
    };
    let modification_time: DateTime<Local> =
      File::open(&file_path).unwrap().metadata().unwrap().modified().unwrap().into();
    assert_eq!(timeval, modification_time);
  }

  #[tokio::test]
  async fn change_file_times_directory_test() {
    let permissions = HashSet::from([UserPermission::Write, UserPermission::Execute]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions);
    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root.join(&dir_name);
    create_dir(&dir_path).expect("Test directory should exist");

    let _cleanup = DirCleanup::new(&dir_path);

    assert!(dir_path.exists());
    let timeval = Local::now().sub(TimeDelta::hours(4));
    let new_time = FileTimes::new().set_modified(timeval.into());
    let result = view.change_file_times(new_time, &dir_name).await;
    let Err(IoError::NotAFileError) = result else {
      panic!("Expected NotAFile Error, got: {:?}", result);
    };
  }

  #[test]
  fn list_dir_current_test() {
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);
    let root = current_dir().unwrap();
    let label = "test";
    let mut view = FileSystemView::new(root.clone(), label, permissions.clone());
    view.change_working_directory("test_files").unwrap();

    let listing = view.list_dir(".").unwrap();

    validate_listing("test_files", &listing, 4, permissions.len(), 3, 1);
  }

  #[test]
  fn list_dir_no_permission_test() {
    let permissions = HashSet::from([]);
    let root = current_dir().unwrap();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());
    let listing = view.list_dir("test");
    let Err(IoError::PermissionError) = listing else {
      panic!("Expected Permission Error, got: {:?}", listing);
    };
  }

  #[test]
  fn list_dir_relative_test() {
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);
    let root = current_dir().unwrap();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());

    let listing = view.list_dir("test_files").unwrap();

    validate_listing("test_files", &listing, 4, permissions.len(), 3, 1);
  }

  #[test]
  fn list_dir_relative_multi_empty_test() {
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);
    let root = current_dir().unwrap();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());

    let listing = view.list_dir("test_files/subfolder").unwrap();

    validate_listing("subfolder", &listing, 1, permissions.len(), 0, 0);
  }

  #[test]
  fn list_dir_absolute_test() {
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);
    let root = current_dir().unwrap();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());

    let listing = view.list_dir("/test_files").unwrap();

    validate_listing("test_files", &listing, 4, permissions.len(), 3, 1);
  }

  #[test]
  fn list_dir_absolute_multi_empty_test() {
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);
    let root = current_dir().unwrap();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());

    let listing = view.list_dir("/test_files/subfolder").unwrap();

    validate_listing("subfolder", &listing, 1, permissions.len(), 0, 0);
  }

  #[test]
  fn list_dir_relative_nonexistent_test() {
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);
    let root = current_dir().unwrap();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());

    let listing = view.list_dir("NONEXISTENT");
    assert!(listing.is_err());
    let Err(IoError::NotFoundError(_)) = listing else {
      panic!("Expected NotFound error");
    };
  }

  #[test]
  fn list_dir_absolute_nonexistent_test() {
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);
    let root = current_dir().unwrap();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());

    let listing = view.list_dir("/NONEXISTENT");
    assert!(listing.is_err());
    let Err(IoError::NotFoundError(_)) = listing else {
      panic!("Expected NotFound error");
    };
  }

  #[test]
  fn list_dir_fs_root_test() {
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);
    let root = current_dir().unwrap().ancestors().last().unwrap().to_path_buf();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());

    let listing = view.list_dir("").unwrap();
    validate_listing(label, &listing, 1, permissions.len(), 0, 0);
  }

  #[test]
  fn list_dir_parent_test() {
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);
    let root = current_dir().unwrap();
    let label = "test";
    let mut view = FileSystemView::new(root.clone(), label, permissions.clone());
    view.change_working_directory("test_files/subfolder").unwrap();

    let listing = view.list_dir("..").unwrap();

    validate_listing("test_files", &listing, 4, permissions.len(), 3, 1);
  }

  #[test]
  fn list_dir_parent_from_root_test() {
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);
    let root = current_dir().unwrap();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());

    let listing = view.list_dir("..");
    let Err(IoError::InvalidPathError(_)) = listing else {
      panic!("Expected InvalidPath error");
    };
  }

  #[test]
  fn list_dir_root_test() {
    let permissions = HashSet::from([UserPermission::Read, UserPermission::List]);
    let root = current_dir().unwrap();
    let label = "test";
    let mut view = FileSystemView::new(root.clone(), label, permissions.clone());
    view.change_working_directory("test_files/subfolder").unwrap();

    let listing = view.list_dir("/").unwrap();

    validate_listing(label, &listing, 9, permissions.len(), 4, 4);
  }

  #[test]
  fn create_dir_relative_test() {
    let permissions = HashSet::from([UserPermission::Create]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());

    let path = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = temp_dir().join(&path);
    let _cleanup = DirCleanup::new(&dir_path);

    let result = view.create_directory(&path);
    assert!(result.is_ok());
    assert_eq!(format!("/{}/{}", &label, &path), result.unwrap());
  }

  #[test]
  fn create_dir_relative_invalid_test() {
    let permissions = HashSet::from([UserPermission::Create]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());

    let path = "..";

    let result = view.create_directory(path);
    let Err(IoError::InvalidPathError(_)) = result else {
      panic!("Expected InvalidPath error, Got: {:?}", result);
    };
  }

  #[test]
  fn create_dir_relative_multi_test() {
    let permissions = HashSet::from([UserPermission::Create]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());

    let path_root = Uuid::new_v4().as_hyphenated().to_string();
    let path = format!("{}/{}", &path_root, Uuid::new_v4().as_hyphenated());
    let dir_path = temp_dir().join(&path_root);
    let _cleanup = DirCleanup::new(&dir_path);

    let result = view.create_directory(&path);
    assert!(result.is_ok());
    assert_eq!(format!("/{}/{}", label, &path), result.unwrap());
    assert!(dir_path.exists());
  }

  #[test]
  fn create_dir_absolute_test() {
    let permissions = HashSet::from([UserPermission::Create]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());

    let path = format!("/{}", Uuid::new_v4().as_hyphenated());
    let dir_path = temp_dir().join(&path[1..]);
    let _cleanup = DirCleanup::new(&dir_path);

    let result = view.create_directory(&path);
    assert!(result.is_ok());
    assert_eq!(format!("/{}{}", &label, &path), result.unwrap());
    assert!(dir_path.exists());
  }

  #[test]
  fn create_dir_absolute_multi_test() {
    let permissions = HashSet::from([UserPermission::Create]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());

    let path_root = Uuid::new_v4().as_hyphenated().to_string();
    let path = format!("/{}/{}", path_root, Uuid::new_v4().as_hyphenated());
    let dir_path = temp_dir().join(&path_root);
    let _cleanup = DirCleanup::new(&dir_path);

    let result = view.create_directory(&path);
    assert!(result.is_ok());
    assert_eq!(format!("/{}{}", &label, &path), result.unwrap());
    assert!(dir_path.exists());
  }

  #[test]
  fn create_dir_then_cwd_unicode_absolute_multi_test() {
    let permissions = HashSet::from([UserPermission::Read, UserPermission::Create]);
    let root = temp_dir();
    let label = "test";
    let mut view = FileSystemView::new(root.clone(), label, permissions);

    let path_root = "测试目录";
    let path = format!("/{}/测试子目录", path_root);
    let dir_path = temp_dir().join(path_root);
    let _cleanup = DirCleanup::new(&dir_path);

    let result = view.create_directory(&path);
    assert!(result.is_ok());
    assert_eq!(format!("/{}{}", &label, &path), result.unwrap());
    assert!(dir_path.exists());

    assert!(view.change_working_directory(path_root).unwrap());
    assert!(view.change_working_directory("..").unwrap());
    assert_eq!(format!("/{label}"), view.display_path);
    assert_eq!(root.clone().canonicalize().unwrap(), view.current_path);
    assert_eq!(root.canonicalize().unwrap(), view.root);
  }

  #[test]
  fn create_dir_no_permission_test() {
    let permissions = HashSet::from([]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());

    let path = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = temp_dir().join(&path);
    let _cleanup = DirCleanup::new(&dir_path);

    let result = view.create_directory(&path);
    let Err(IoError::PermissionError) = result else {
      panic!("Expected Permission Error, got: {:?}", result);
    };
    assert!(!dir_path.exists());
  }

  #[test]
  fn create_dir_absolute_with_label_test() {
    let permissions = HashSet::from([UserPermission::Create]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());

    let path = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = temp_dir().join(&path);
    let _cleanup = DirCleanup::new(&dir_path);

    let new_path = format!("/{}/{}", &label, &path);
    let result = view.create_directory(&new_path);
    assert!(result.is_ok());
    assert_eq!(new_path, result.unwrap());
    assert!(dir_path.exists());
  }

  #[test]
  fn create_dir_absolute_multi_with_label_test() {
    let permissions = HashSet::from([UserPermission::Create]);
    let root = temp_dir();
    let label = "test";
    let view = FileSystemView::new(root.clone(), label, permissions.clone());

    let path_root = Uuid::new_v4().as_hyphenated().to_string();
    let path = format!("/{}/{}", path_root, Uuid::new_v4().as_hyphenated());
    let dir_path = temp_dir().join(&path_root);
    let _cleanup = DirCleanup::new(&dir_path);

    let result = view.create_directory(&format!("/{}{}", &label, &path));
    assert!(result.is_ok());
    assert_eq!(format!("/{}{}", &label, &path), result.unwrap());
    assert!(dir_path.exists());
  }

  pub(crate) fn validate_listing(
    listed_dir_name: &str,
    listing: &Vec<EntryData>,
    total: usize,
    perms: usize,
    files: usize,
    dirs: usize,
  ) {
    assert!(listing.len() >= total);
    let mut cdir_count = 0;
    let mut file_count = 0;
    let mut dir_count = 0;
    for entry in listing {
      if entry.entry_type() == EntryType::Cdir {
        assert_eq!(listed_dir_name, entry.name());
        cdir_count += 1;
      } else if entry.entry_type() == EntryType::Dir {
        dir_count += 1;
      } else if entry.entry_type() == EntryType::File {
        file_count += 1;
      }
      assert!(!entry.name().is_empty());
      assert!(entry.perm().len() <= perms); // TODO better permission check
    }

    assert_eq!(1, cdir_count);
    assert!(file_count >= files);
    assert!(dir_count >= dirs);
  }
}
