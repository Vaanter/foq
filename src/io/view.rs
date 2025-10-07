use crate::auth::user_permission::UserPermission;
use crate::io::entry_data::EntryData;
use crate::io::error::IoError;
use crate::io::open_options_flags::OpenOptionsWrapper;
use async_trait::async_trait;
use path_clean::PathClean;
use std::collections::HashSet;
use std::fs::FileTimes;
use std::path::{Path, PathBuf};
use tokio::fs::{File, OpenOptions};
use tracing::{debug, trace, warn};

#[async_trait]
pub(crate) trait View {
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
  fn change_working_directory(&mut self, path: &str) -> Result<bool, IoError>;
  fn create_directory(&self, path: &str) -> Result<String, IoError>;
  /// Opens a file with the specified path and options.
  ///
  /// This function asynchronously opens a file using the provided `path` and `options`, and
  /// returns a `Result` containing the opened [`File`] or an [`IoError`] if an error occurs.
  ///
  /// # Arguments
  ///
  /// - `path`: A type that can be converted into a [`String`], representing the path of the file
  /// to be opened.
  /// - `options`: An [`OpenOptionsWrapper`] containing the options for opening the file.
  ///
  /// # Returns
  ///
  /// A [`Result`] containing the opened [`File`] if successful, or an [`IoError`] if an error
  /// occurs.
  ///
  /// # Errors
  ///
  /// This function can return the following [`IoError`] variants:
  ///
  /// - [`IoError::PermissionError`]: If the requested operation is not permitted based on the users
  /// permissions.
  /// - [`IoError::NotFoundError`]: If the file specified by the `path` does not exist.
  /// - [`IoError::OsError`]: If an operating system error occurs during the file opening process.
  /// - [`IoError::NotAFileError`]: If the specified `path` refers to a directory instead of a file.
  ///
  async fn open_file(&self, path: &str, options: OpenOptionsWrapper) -> Result<File, IoError> {
    if options.read && !self.get_permissions().contains(&UserPermission::Read)
      || (options.write && !self.get_permissions().contains(&UserPermission::Write))
      || (options.create && !self.get_permissions().contains(&UserPermission::Create))
      || (options.append && !self.get_permissions().contains(&UserPermission::Append))
      || (options.truncate && !self.get_permissions().contains(&UserPermission::Write))
    {
      return Err(IoError::PermissionError);
    }

    let path = self.process_path(path).clean();

    if !path.starts_with(self.get_root_path()) {
      return Err(IoError::InvalidPathError(String::from("Invalid path!")));
    }

    if path.is_dir() {
      return Err(IoError::NotAFileError);
    }

    debug!("Opening: {:?}", &path);

    OpenOptions::from(options).open(&path).await.map_err(|e| {
      warn!("Error opening file: {}", e);
      IoError::map_io_error(e)
    })
  }
  async fn delete_file(&self, path: &str) -> Result<(), IoError>;
  async fn delete_folder(&self, path: &str) -> Result<(), IoError>;
  async fn delete_folder_recursive(&self, path: &str) -> Result<(), IoError>;
  async fn change_file_times(&self, new_time: FileTimes, path: &str) -> Result<(), IoError>;
  /// Creates a directory listing.
  ///
  /// This function lists all files and directories at `path` as [`EntryData`]. If the listing
  /// succeeds, then it is returned, otherwise [`IoError`] is returned.
  ///
  /// # Arguments
  ///
  /// - `path` A type that can be converted into a [`String`], representing the path to directory
  ///   to list.
  ///
  /// # Returns
  ///
  /// A [`Result`] containing the listing as [`Vec<EntryData>`] if successful or an [`IoError`] if
  /// an error occurs.
  ///
  fn list_dir(&self, path: &str) -> Result<Vec<EntryData>, IoError>;
  fn get_label(&self) -> &str;
  fn get_display_path(&self) -> &str;
  fn get_permissions(&self) -> &HashSet<UserPermission>;
  fn get_current_path(&self) -> &Path;
  fn get_root_path(&self) -> &Path;

  ///
  /// Preprocesses path by stripping leading characters and joining with view's root or current_path.
  ///
  /// If the input path is absolute, leading '/' and if present the view's label are stripped.
  /// Relative path just joins with current_path.
  #[inline(always)]
  fn process_path(&self, path: &str) -> PathBuf {
    trace!("Processing path: {}", path);
    if let Some(stripped) = path.strip_prefix(&format!("/{}/", self.get_label())) {
      self.get_root_path().join(stripped)
    } else if let Some(stripped) = path.strip_prefix('/') {
      self.get_root_path().join(stripped)
    } else {
      self.get_current_path().join(path)
    }
  }
}
