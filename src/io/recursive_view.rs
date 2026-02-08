use crate::auth::user_permission::UserPermission;
use crate::io::entry_data::EntryData;
use crate::io::error::IoError;
use crate::io::view::View;
use async_trait::async_trait;
use std::collections::HashSet;
use std::fs::FileTimes;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{debug, warn};
use walkdir::{DirEntry, WalkDir};

#[derive(Clone, Debug)]
pub(crate) struct RecursiveView {
  pub(crate) root: PathBuf,        // native path to starting directory
  pub(crate) display_path: String, // virtual path
  pub(crate) label: String,
  pub(crate) permissions: HashSet<UserPermission>,
  cached_entries: Arc<Mutex<Option<EntriesHolder>>>,
}

#[derive(Clone, Debug)]
struct EntriesHolder {
  created: Instant,
  entries: Vec<DirEntry>,
}

impl PartialEq for RecursiveView {
  fn eq(&self, other: &Self) -> bool {
    self.root == other.root
      && self.display_path == other.display_path
      && self.label == other.label
      && self.permissions == other.permissions
  }
}

impl RecursiveView {
  #[cfg(test)]
  pub(crate) fn new(root: PathBuf, label: &str, permissions: HashSet<UserPermission>) -> Self {
    let label = label.into();
    let root = root.canonicalize().expect("View path must exist!");
    RecursiveView {
      root,
      display_path: format!("/{}", label),
      label,
      permissions,
      cached_entries: Arc::new(Mutex::new(None)),
    }
  }

  /// Creates a new instance of a `RecursiveView`.
  ///
  /// This function takes in a `root` path, a `label`, and a set of `permissions`, and returns a
  /// [`Ok<RecursiveView>`]. If the root path cannot be canonicalized, then this will
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
  /// An [`Result`] containing the new [`RecursiveView`] if successful, or [`Err`] if an error
  /// occurs.
  ///
  pub(crate) fn new_option(
    root: PathBuf,
    label: &str,
    permissions: HashSet<UserPermission>,
  ) -> Result<Self, ()> {
    let label = label.into();
    match root.canonicalize() {
      Ok(r) => Ok(RecursiveView {
        root: r,
        display_path: format!("/{}", label),
        label,
        permissions,
        cached_entries: Arc::new(Mutex::new(None)),
      }),
      Err(_) => Err(()),
    }
  }

  fn map_entries_to_entry_data(&self, entry: &DirEntry) -> Option<EntryData> {
    entry.metadata().ok().and_then(|meta| {
      let file_components = entry.path().components();
      let root_components: HashSet<Component> = self.root.components().collect();
      file_components
        .filter(|comp| !root_components.contains(comp))
        .map(|comp| comp.as_os_str().to_string_lossy().to_string())
        .reduce(|mut acc, comp_name| {
          acc.push('\\');
          acc.push_str(&comp_name);
          acc
        })
        .map(|name| EntryData::create_from_metadata(meta, name, &self.permissions))
    })
  }
}

#[async_trait]
impl View for RecursiveView {
  fn change_working_directory(&mut self, path: &str) -> Result<bool, IoError> {
    // As all files are shown at the same level, this shouldn't even get called
    warn!("{:?}", path);
    Err(IoError::SystemError)
  }

  fn create_directory(&self, _path: &str) -> Result<String, IoError> {
    Err(IoError::InvalidPathError("Directory can't be created here".to_string()))
  }

  async fn delete_file(&self, path: &str) -> Result<(), IoError> {
    Err(IoError::SystemError)
  }

  async fn delete_folder(&self, path: &str) -> Result<(), IoError> {
    // No folders should ever be sent, so this shouldn't even get called
    warn!("{:?}", path);
    Err(IoError::SystemError)
  }

  async fn delete_folder_recursive(&self, path: &str) -> Result<(), IoError> {
    // No folders should ever be sent, so this shouldn't even get called
    warn!("{:?}", path);
    Err(IoError::SystemError)
  }

  fn list_dir(&self, _path: &str) -> Result<Vec<EntryData>, IoError> {
    if !self.permissions.contains(&UserPermission::List) {
      return Err(IoError::PermissionError);
    }

    if let Ok(entries) = &self.cached_entries.try_lock()
      && let Some(entries) = entries.as_ref()
    {
      if entries.created.elapsed().as_secs() < 300 {
        debug!("Using cached entries, elapsed: {:?}", entries.created.elapsed());
        // TODO optimize
        return Ok(
          entries
            .entries
            .iter()
            .filter_map(|entry| self.map_entries_to_entry_data(entry))
            .collect(),
        );
      } else {
        debug!("Cache miss, entries too old: {:?}", entries.created.elapsed());
      }
    }

    let mut new_cached = Vec::new();
    let entries: Vec<EntryData> = WalkDir::new(&self.root)
      .follow_links(false)
      .same_file_system(true)
      .into_iter()
      .filter_map(|entry| entry.ok())
      .filter(|entry| entry.file_type().is_file())
      .inspect(|entry| new_cached.push(entry.clone()))
      .filter_map(|entry| self.map_entries_to_entry_data(&entry))
      .collect();
    if let Ok(mut cached) = self.cached_entries.try_lock() {
      let _ = cached.insert(EntriesHolder {
        created: Instant::now(),
        entries: new_cached,
      });
    }

    Ok(entries)
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
    &self.root
  }

  fn get_root_path(&self) -> &Path {
    &self.root
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::utils::test_utils::{DirCleanup, create_dir, touch};
  use std::env::{current_dir, temp_dir};
  use uuid::Uuid;

  #[test]
  fn list_dir_sanity_test() {
    let permissions = HashSet::from([UserPermission::List]);
    let root = current_dir().unwrap().join("test_files");
    let label = "test";
    let view = RecursiveView::new(root.clone(), label, permissions.clone());
    let listing = view.list_dir("test").unwrap();
    assert_eq!(4, listing.len());
  }

  #[test]
  fn list_dir_no_permission_test() {
    let permissions = HashSet::from([]);
    let root = current_dir().unwrap().join("test_files");
    let label = "test";
    let view = RecursiveView::new(root.clone(), label, permissions.clone());
    let listing = view.list_dir("test");
    let Err(IoError::PermissionError) = listing else {
      panic!("Expected Permission Error, got: {:?}", listing);
    };
  }

  #[test]
  fn list_dir_cache_test() {
    let permissions = HashSet::from([UserPermission::List]);
    let root = current_dir().unwrap().join("test_files");
    let label = "test";
    let view = RecursiveView::new(root.clone(), label, permissions.clone());
    let listing = view.list_dir("test");
    let cached = view.cached_entries.try_lock().unwrap();
    assert!(cached.is_some());
    assert_eq!(listing.unwrap().len(), cached.as_ref().unwrap().entries.len());
  }

  #[test]
  fn map_entries_to_entry_data_same_components_test() {
    let permissions = HashSet::from([UserPermission::List]);
    let root = temp_dir();
    let dir_name = Uuid::new_v4().as_hyphenated().to_string();
    let dir_path = root.join(&dir_name);
    create_dir(&dir_path).expect("Test directory should exist");
    DirCleanup::new(&dir_path);
    let same_component = "same_component";
    let sub_path = dir_path.join(same_component).join(same_component);
    create_dir(&sub_path).unwrap();

    let files = (1..5).map(|_| Uuid::new_v4().as_hyphenated().to_string()).collect::<Vec<_>>();
    for file in files.iter() {
      touch(&sub_path.join(file)).expect("Test file should exist");
    }

    let label = "test";
    let view = RecursiveView::new(dir_path.clone(), label, permissions);
    let listing: Vec<DirEntry> =
      WalkDir::new(&sub_path).into_iter().filter_map(|e| e.ok()).collect();
    for entry in &listing {
      let entry_data = view.map_entries_to_entry_data(entry).unwrap();
      let name = entry_data.name().to_string();
      assert_eq!(2, name.matches(&same_component).count(), "name: {}", &name);
    }
  }
}
