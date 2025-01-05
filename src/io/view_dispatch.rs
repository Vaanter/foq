use crate::auth::user_permission::UserPermission;
use crate::io::entry_data::EntryData;
use crate::io::error::IoError;
use crate::io::file_system_view::FileSystemView;
use crate::io::open_options_flags::OpenOptionsWrapper;
use crate::io::view::View;
use async_trait::async_trait;
use std::collections::HashSet;
use std::fs::FileTimes;
use std::path::Path;
use tokio::fs::File;

#[derive(Debug, PartialEq, Clone)]
pub(crate) enum ViewDispatch {
  FileSystemView(FileSystemView),
}

impl From<FileSystemView> for ViewDispatch {
  fn from(view: FileSystemView) -> Self {
    ViewDispatch::FileSystemView(view)
  }
}

#[async_trait]
impl View for ViewDispatch {
  fn change_working_directory(&mut self, path: &str) -> Result<bool, IoError> {
    match self {
      ViewDispatch::FileSystemView(v) => v.change_working_directory(path),
    }
  }

  fn create_directory(&self, path: &str) -> Result<String, IoError> {
    match self {
      ViewDispatch::FileSystemView(v) => v.create_directory(path),
    }
  }

  async fn open_file(&self, path: &str, options: OpenOptionsWrapper) -> Result<File, IoError> {
    match self {
      ViewDispatch::FileSystemView(v) => v.open_file(path, options).await,
    }
  }

  async fn delete_file(&self, path: &str) -> Result<(), IoError> {
    match self {
      ViewDispatch::FileSystemView(v) => v.delete_file(path).await,
    }
  }

  async fn delete_folder(&self, path: &str) -> Result<(), IoError> {
    match self {
      ViewDispatch::FileSystemView(v) => v.delete_folder(path).await,
    }
  }

  async fn delete_folder_recursive(&self, path: &str) -> Result<(), IoError> {
    match self {
      ViewDispatch::FileSystemView(v) => v.delete_folder_recursive(path).await,
    }
  }

  async fn change_file_times(&self, new_time: FileTimes, path: &str) -> Result<(), IoError> {
    match self {
      ViewDispatch::FileSystemView(v) => v.change_file_times(new_time, path).await,
    }
  }

  fn list_dir(&self, path: &str) -> Result<Vec<EntryData>, IoError> {
    match self {
      ViewDispatch::FileSystemView(v) => v.list_dir(path),
    }
  }

  fn get_label(&self) -> &str {
    match self {
      ViewDispatch::FileSystemView(v) => v.get_label(),
    }
  }

  fn get_display_path(&self) -> &str {
    match self {
      ViewDispatch::FileSystemView(v) => v.get_display_path(),
    }
  }

  fn get_permissions(&self) -> &HashSet<UserPermission> {
    match self {
      ViewDispatch::FileSystemView(v) => v.get_permissions(),
    }
  }

  fn get_current_path(&self) -> &Path {
    match self {
      ViewDispatch::FileSystemView(v) => v.get_current_path(),
    }
  }

  fn get_root_path(&self) -> &Path {
    match self {
      ViewDispatch::FileSystemView(v) => v.get_root_path(),
    }
  }
}
