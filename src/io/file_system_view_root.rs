use std::collections::BTreeMap;

use tokio::fs::File;
use tracing::debug;

use crate::auth::user_permission::UserPermission;
use crate::io::entry_data::{EntryData, EntryType};
use crate::io::error::Error;
use crate::io::file_system_view::FileSystemView;
use crate::io::open_options_flags::OpenOptionsWrapper;

#[derive(Debug, Default)]
pub(crate) struct FileSystemViewRoot {
  pub(crate) file_system_views: Option<BTreeMap<String, FileSystemView>>,
  current_view: Option<String>,
}

const ROOT_PERMISSIONS: [UserPermission; 2] = [UserPermission::EXECUTE, UserPermission::LIST];

impl FileSystemViewRoot {
  pub(crate) fn new(views: Option<BTreeMap<String, FileSystemView>>) -> Self {
    FileSystemViewRoot {
      file_system_views: views,
      current_view: None,
    }
  }

  pub(crate) fn set_views(&mut self, view: Vec<FileSystemView>) {
    let views = view
      .into_iter()
      .map(|v| (v.label.clone(), v))
      .collect();
    self.file_system_views = Some(views);
  }

  // TODO better return
  pub(crate) fn change_working_directory(&mut self, path: impl Into<String>) -> bool {
    let path = path.into();
    if path == "." || path.is_empty() {
      return false;
    } else if path == ".." {
      if self.file_system_views.is_none() || self.current_view.is_none() {
        return false;
      }

      let view = self
        .file_system_views
        .as_mut()
        .unwrap()
        .get_mut(self.current_view.as_ref().unwrap());
      if view.is_none() {
        return false;
      }

      return if view.unwrap().change_working_directory_up() {
        true
      } else {
        self.current_view = None;
        true
      };
    } else if path == "~" || path == "/" {
      let mut changed = false;
      if let (Some(view), Some(views)) = (&self.current_view, self.file_system_views.as_mut()) {
        views.get_mut(view).unwrap().change_working_directory("/");
        changed = true;
        self.current_view = None;
      }
      return changed;
    } else if path.starts_with("/") {
      let label = path.split("/").nth(1);
      if label.is_none() || self.file_system_views.is_none() {
        return false;
      }
      let label = label.unwrap();

      let view = self.file_system_views.as_mut().unwrap().get_mut(label);
      if view.is_none() {
        return false;
      }
      let mut sub_path = path.split("/").skip(2).collect::<Vec<&str>>().join("/");
      sub_path.insert(0, '/');
      let changed = view.unwrap().change_working_directory(sub_path);
      if changed {
        self.current_view.replace(label.to_string());
      }
      changed
    } else {
      if self.file_system_views.is_none() {
        return false;
      }
      if self.current_view.is_none() {
        let label = path.split("/").nth(0);
        if label.is_none() {
          return false;
        }
        let label = label.unwrap();
        let view = self.file_system_views.as_mut().unwrap().get_mut(label);
        if view.is_none() {
          return false;
        }
        let sub_path = path.split("/").skip(1).collect::<Vec<&str>>().join("/");
        let changed = view.unwrap().change_working_directory(sub_path);
        if changed {
          self.current_view.replace(label.to_string());
        }
        return changed;
      }
      return self
        .file_system_views
        .as_mut()
        .unwrap()
        .get_mut(self.current_view.as_ref().unwrap())
        .as_mut()
        .unwrap()
        .change_working_directory(path);
    }
  }

  pub(crate) fn change_working_directory_up(&mut self) -> bool {
    self.change_working_directory("..")
  }

  #[tracing::instrument(skip(self))]
  pub(crate) fn get_current_working_directory(&self) -> String {
    debug!("Getting current working directory path");
    if self.current_view.is_none() || self.file_system_views.is_none() {
      return String::from("/");
    }

    let current_view = self.current_view.as_ref().unwrap();
    return format!(
      "{}",
      self
        .file_system_views
        .as_ref()
        .unwrap()
        .get(current_view)
        .unwrap()
        .display_path
    );
  }

  #[tracing::instrument(skip(self, path))]
  pub(crate) fn list_dir(&self, path: impl Into<String>) -> Result<Vec<EntryData>, Error> {
    let path = path.into();
    debug!("Listing directory, path: {}", path);
    if self.file_system_views.is_none() {
      // not logged in
      return Err(Error::UserError);
    }

    if path.is_empty() || path == "." {
      if self.current_view.is_none() {
        self.list_root()
      } else {
        // list current view
        let view = self
          .file_system_views
          .as_ref()
          .unwrap()
          .get(self.current_view.as_ref().unwrap());

        if view.is_none() {
          // Current view doesn't exist (should panic?)
          return Err(Error::SystemError);
        }

        view.unwrap().list_dir(".")
      }
    } else if path == "/" || path == "~" {
      self.list_root()
    } else if path == ".." {
      if self.current_view.is_none() {
        // We are at root, nothing is before
        return Err(Error::InvalidPathError(String::from(
          "Parent path is inaccessible!",
        )));
      }

      let view = self
        .file_system_views
        .as_ref()
        .unwrap()
        .get(self.current_view.as_ref().unwrap());

      if view.is_none() {
        // Current view doesn't exist (should panic?)
        return Err(Error::SystemError);
      }

      let listing = view.unwrap().list_dir("..");

      return match listing {
        Ok(l) => Ok(l),
        Err(Error::InvalidPathError(_)) => self.list_root(),
        Err(e) => Err(e),
      };
    } else if path.starts_with("/") {
      // list absolute
      let label = path.split("/").nth(1);
      if label.is_none() {
        // path is invalid (e.g.: //foo/bar)
        return Err(Error::InvalidPathError(String::from("Invalid path!")));
      }

      let view = self.file_system_views.as_ref().unwrap().get(label.unwrap());

      if view.is_none() {
        // Current view doesn't exist (should panic?)
        return Err(Error::SystemError);
      }

      let mut sub_path = path.split("/").skip(2).collect::<Vec<&str>>().join("/");
      sub_path.insert(0, '/');
      view.unwrap().list_dir(sub_path)
    } else {
      // list relative
      if self.current_view.is_none() {
        // relative of root
        let label = path.split("/").nth(0).expect("Path cannot be empty here!");
        let view = self.file_system_views.as_ref().unwrap().get(label);
        if view.is_none() {
          return Err(Error::NotFoundError(String::from("Path doesn't exist!")));
        }
        let sub_path = path.split("/").skip(1).collect::<Vec<&str>>().join("/");
        return view.unwrap().list_dir(sub_path);
      }

      // relative of view
      let view = self
        .file_system_views
        .as_ref()
        .unwrap()
        .get(self.current_view.as_ref().unwrap());

      if view.is_none() {
        // Current view doesn't exist (should panic?)
        return Err(Error::SystemError);
      }

      view.unwrap().list_dir(path)
    }
  }

  fn list_root(&self) -> Result<Vec<EntryData>, Error> {
    if self.file_system_views.is_none() {
      return Err(Error::UserError);
    }

    let mut entries: Vec<EntryData> =
      Vec::with_capacity(self.file_system_views.as_ref().unwrap().len() + 1);
    entries.push(EntryData::new(
      0,
      EntryType::CDIR,
      ROOT_PERMISSIONS.to_vec(),
      String::new(),
      "/",
    ));
    entries.extend(self.file_system_views.as_ref().unwrap().iter().map(|v| {
      EntryData::new(
        0,
        EntryType::DIR,
        v.1.permissions.iter().cloned().collect(),
        String::new(),
        v.1.label.clone(),
      )
    }));

    Ok(entries)
  }

  #[tracing::instrument(skip(self, path, options))]
  pub(crate) async fn open_file(
    &self,
    path: impl Into<String>,
    options: OpenOptionsWrapper,
  ) -> Result<File, Error> {
    let path = path.into();
    if self.file_system_views.is_none() {
      return Err(Error::UserError);
    }

    debug!("Opening file: {}.", path);
    if path.is_empty() || path == "/" {
      return Err(Error::InvalidPathError(String::from(
        "Path references a directory, not a file!",
      )));
    } else if path.starts_with("/") {
      let label = path.split("/").nth(1).expect("Path cannot be empty here!");
      let view = self.file_system_views.as_ref().unwrap().get(label);

      if view.is_none() {
        return Err(Error::NotFoundError(String::from("File not found!")));
      }

      let mut sub_path = path.split("/").skip(2).collect::<Vec<&str>>().join("/");
      sub_path.insert(0, '/');
      return view.unwrap().open_file(sub_path, options).await;
    } else {
      if self.current_view.is_none() {
        // relative of root
        let label = path.split("/").nth(0).expect("Path cannot be empty here!");
        let view = self.file_system_views.as_ref().unwrap().get(label);
        if view.is_none() {
          return Err(Error::InvalidPathError(String::from("Path doesn't exist!")));
        }
        let sub_path = path.split("/").skip(1).collect::<Vec<&str>>().join("/");
        return view.unwrap().open_file(sub_path, options).await;
      }
      return self
        .file_system_views
        .as_ref()
        .unwrap()
        .get(self.current_view.as_ref().unwrap())
        .unwrap()
        .open_file(path, options)
        .await;
    };
  }
}

#[cfg(test)]
mod tests {
  use std::collections::{BTreeMap, HashSet};

  use crate::auth::user_permission::UserPermission;
  use crate::io::entry_data::EntryData;
  use crate::io::error::Error;
  use crate::io::file_system_view::tests::validate_listing;
  use crate::io::file_system_view::FileSystemView;
  use crate::io::file_system_view_root::FileSystemViewRoot;
  use crate::io::open_options_flags::OpenOptionsWrapperBuilder;

  #[tokio::test]
  async fn open_file_not_logged_in_test() {
    let root = FileSystemViewRoot::new(None);

    let options = OpenOptionsWrapperBuilder::default()
      .read(true)
      .build()
      .unwrap();

    let file = root.open_file("test_file", options).await;
    let Err(Error::UserError) = file else {
      panic!("Expected User error");
    };
  }

  #[tokio::test]
  async fn open_file_absolute_test() {
    let permissions = HashSet::from([UserPermission::READ]);

    let mut root1 = std::env::current_dir().unwrap();
    let mut root2 = std::env::current_dir().unwrap();
    root1.push("src");
    root2.push("test_files");
    let label1 = "src";
    let label2 = "test_files";
    let view1 = FileSystemView::new(root1, label1.clone(), permissions.clone());
    let view2 = FileSystemView::new(root2, label2.clone(), permissions.clone());
    let views = create_root(vec![view1, view2]);

    let options = OpenOptionsWrapperBuilder::default()
      .read(true)
      .build()
      .unwrap();
    let root = FileSystemViewRoot::new(Some(views));
    let file = root.open_file(format!("/{label2}/2KiB.txt"), options).await;
    assert!(file.is_ok());
  }

  #[tokio::test]
  async fn open_file_relative_test() {
    let permissions = HashSet::from([UserPermission::READ]);

    let mut root1 = std::env::current_dir().unwrap();
    let mut root2 = std::env::current_dir().unwrap();
    root1.push("src");
    root2.push("test_files");
    let label1 = "src";
    let label2 = "test_files";
    let view1 = FileSystemView::new(root1, label1.clone(), permissions.clone());
    let view2 = FileSystemView::new(root2, label2.clone(), permissions.clone());
    let views = create_root(vec![view1, view2]);

    let options = OpenOptionsWrapperBuilder::default()
      .read(true)
      .build()
      .unwrap();
    let root = FileSystemViewRoot::new(Some(views));
    let file = root.open_file(format!("{label2}/2KiB.txt"), options).await;
    assert!(file.is_ok());
  }

  #[tokio::test]
  async fn open_file_relative_nonexistent_test() {
    let permissions = HashSet::from([UserPermission::READ]);

    let mut root1 = std::env::current_dir().unwrap();
    let mut root2 = std::env::current_dir().unwrap();
    root1.push("src");
    root2.push("test_files");
    let label1 = "src";
    let label2 = "test_files";
    let view1 = FileSystemView::new(root1, label1.clone(), permissions.clone());
    let view2 = FileSystemView::new(root2, label2.clone(), permissions.clone());
    let views = create_root(vec![view1, view2]);

    let options = OpenOptionsWrapperBuilder::default()
      .read(true)
      .build()
      .unwrap();
    let root = FileSystemViewRoot::new(Some(views));
    let file = root
      .open_file(format!("{label2}/NONEXISTENT"), options)
      .await;
    let Err(Error::NotFoundError(_)) = file else {
      panic!("Expected NotFound error, got: {:?}", file);
    };
  }

  #[test]
  fn get_cwd_not_logged_in() {
    let root = FileSystemViewRoot::new(None);

    let cwd = root.get_current_working_directory();
    assert_eq!(String::from("/"), cwd);
  }

  #[test]
  fn get_cwd_with_view() {
    let permissions = HashSet::from([UserPermission::READ]);
    let root1 = std::env::current_dir().unwrap();
    let label = "current_dir";
    let view1 = FileSystemView::new(root1.clone(), label.clone(), permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    assert!(root.change_working_directory(format!("{}/test_files", label.clone())));
    assert_eq!(
      root.get_current_working_directory(),
      format!("/{}/test_files", label.clone())
    );
  }

  #[test]
  fn cwd_to_root_test() {
    let permissions = HashSet::from([UserPermission::READ]);
    let root1 = std::env::current_dir().unwrap();
    let label = "current_dir";
    let view1 = FileSystemView::new(root1.clone(), label.clone(), permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    assert!(root.change_working_directory(format!("{}/test_files", label.clone())));
    assert!(root.change_working_directory("~"));
    assert!(root.current_view.is_none());
  }

  #[test]
  fn cwd_to_file_system_from_root_test() {
    let permissions = HashSet::from([UserPermission::READ]);
    let root1 = std::env::current_dir().unwrap();
    let label = "current_dir";
    let view1 = FileSystemView::new(root1.clone(), label.clone(), permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    assert!(root.change_working_directory(label.clone()));
    assert!(root.current_view.is_some());
    assert_eq!(root.current_view.unwrap(), label.clone());
    assert_eq!(
      root
        .file_system_views
        .unwrap()
        .get(label)
        .unwrap()
        .display_path,
      format!("/{label}")
    );
  }

  #[test]
  fn cwd_to_file_system_from_root_relative_multi_test() {
    let permissions = HashSet::from([UserPermission::READ]);
    let root1 = std::env::current_dir().unwrap();
    let label = "current_dir";
    let view1 = FileSystemView::new(root1.clone(), label.clone(), permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    assert!(root.change_working_directory(format!("{label}/test_files")));
    assert!(root.current_view.is_some());
    assert_eq!(root.current_view.unwrap(), label.clone());
    assert_eq!(
      root
        .file_system_views
        .unwrap()
        .get(label)
        .unwrap()
        .display_path,
      format!("/{label}/test_files")
    );
  }

  #[test]
  fn cwd_to_file_system_from_root_absolute_multi_test() {
    let permissions = HashSet::from([UserPermission::READ]);
    let root1 = std::env::current_dir().unwrap();
    let label = "current_dir";
    let view1 = FileSystemView::new(root1.clone(), label.clone(), permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    assert!(root.change_working_directory(format!("/{label}/test_files")));
    assert!(root.current_view.is_some());
    assert_eq!(root.current_view.unwrap(), label.clone());
    assert_eq!(
      root
        .file_system_views
        .unwrap()
        .get(label)
        .unwrap()
        .display_path,
      format!("/{label}/test_files")
    );
  }

  #[test]
  fn cwd_from_current_relative_test() {
    let permissions = HashSet::from([UserPermission::READ]);
    let root1 = std::env::current_dir().unwrap();
    let label = "current_dir";
    let view1 = FileSystemView::new(root1.clone(), label.clone(), permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    assert!(root.change_working_directory(format!("/{label}")));
    assert!(root.change_working_directory("test_files"));
    assert!(root.current_view.is_some());
    assert_eq!(root.current_view.unwrap(), label.clone());
    assert_eq!(
      root
        .file_system_views
        .unwrap()
        .get(label)
        .unwrap()
        .display_path,
      format!("/{label}/test_files")
    );
  }

  #[test]
  fn list_dir_not_logged_in_empty_test() {
    let root = FileSystemViewRoot::new(None);

    let file = root.list_dir("");
    let Err(Error::UserError) = file else {
      panic!("Expected User error")
    };
  }

  #[test]
  fn list_dir_not_logged_in_relative_test() {
    let root = FileSystemViewRoot::new(None);

    let file = root.list_dir("test_files");
    let Err(Error::UserError) = file else {
      panic!("Expected User error")
    };
  }

  #[test]
  fn list_dir_not_logged_in_absolute_test() {
    let root = FileSystemViewRoot::new(None);

    let file = root.list_dir("/test_files");
    let Err(Error::UserError) = file else {
      panic!("Expected User error")
    };
  }

  #[test]
  fn list_dir_not_logged_in_parent_test() {
    let root = FileSystemViewRoot::new(None);

    let file = root.list_dir("..");
    let Err(Error::UserError) = file else {
      panic!("Expected User error");
    };
  }

  #[test]
  fn list_dir_root_test() {
    let permissions = HashSet::from([UserPermission::READ, UserPermission::LIST]);

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
    assert_eq!(
      HashSet::<&EntryData>::from_iter(listing.iter()).len(),
      listing.len()
    );
    validate_listing(&listing, 3, 2, 0, 2);
  }

  #[test]
  fn list_dir_root_dot_test() {
    let permissions = HashSet::from([UserPermission::READ, UserPermission::LIST]);

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
    assert_eq!(
      HashSet::<&EntryData>::from_iter(listing.iter()).len(),
      listing.len()
    );
    validate_listing(&listing, 3, 2, 0, 2);
  }

  #[test]
  fn list_dir_root_parent_test() {
    let permissions = HashSet::from([UserPermission::READ, UserPermission::LIST]);

    let mut root1 = std::env::current_dir().unwrap();
    root1.push("test_files");
    let label = "test_files";
    let view1 = FileSystemView::new(root1, label.clone(), permissions.clone());
    let views = create_root(vec![view1]);

    let root = FileSystemViewRoot::new(Some(views));
    let listing = root.list_dir("..");
    let Err(Error::InvalidPathError(_)) = listing else {
      panic!("Expected InvalidPath error");
    };
  }

  #[test]
  fn list_dir_current_test() {
    let permissions = HashSet::from([UserPermission::READ, UserPermission::LIST]);

    let mut root1 = std::env::current_dir().unwrap();
    let mut root2 = std::env::current_dir().unwrap();
    root1.push("src");
    root2.push("test_files");
    let label1 = "src";
    let label2 = "test_files";
    let view1 = FileSystemView::new(root1, label1.clone(), permissions.clone());
    let view2 = FileSystemView::new(root2, label2.clone(), permissions.clone());
    let views = create_root(vec![view1, view2]);

    let mut root = FileSystemViewRoot::new(Some(views));
    root.change_working_directory("test_files");

    let listing = root.list_dir(".").unwrap();

    validate_listing(&listing, 5, permissions.len(), 3, 1);
  }

  #[test]
  fn list_dir_relative_empty_test() {
    let permissions = HashSet::from([UserPermission::READ, UserPermission::LIST]);

    let mut root1 = std::env::current_dir().unwrap();
    let mut root2 = std::env::current_dir().unwrap();
    root1.push("src");
    root2.push("test_files");
    let label1 = "src";
    let label2 = "test_files";
    let view1 = FileSystemView::new(root1, label1.clone(), permissions.clone());
    let view2 = FileSystemView::new(root2, label2.clone(), permissions.clone());
    let views = create_root(vec![view1, view2]);

    let mut root = FileSystemViewRoot::new(Some(views));
    root.change_working_directory("test_files");

    let listing = root.list_dir("subfolder").unwrap();

    validate_listing(&listing, 1, permissions.len(), 0, 0);
  }

  #[test]
  fn list_dir_absolute_test() {
    let permissions = HashSet::from([UserPermission::READ, UserPermission::LIST]);

    let mut root1 = std::env::current_dir().unwrap();
    let mut root2 = std::env::current_dir().unwrap();
    root1.push("src");
    root2.push("test_files");
    let label1 = "src";
    let label2 = "test_files";
    let view1 = FileSystemView::new(root1, label1.clone(), permissions.clone());
    let view2 = FileSystemView::new(root2, label2.clone(), permissions.clone());
    let views = create_root(vec![view1, view2]);

    let root = FileSystemViewRoot::new(Some(views));

    let listing = root.list_dir("/test_files").unwrap();

    validate_listing(&listing, 5, permissions.len(), 3, 1);
  }

  #[test]
  fn list_dir_root_from_view_parent_test() {
    let permissions = HashSet::from([UserPermission::READ, UserPermission::LIST]);

    let mut root1 = std::env::current_dir().unwrap();
    let mut root2 = std::env::current_dir().unwrap();
    root1.push("src");
    root2.push("test_files");
    let label1 = "src";
    let label2 = "test_files";
    let view1 = FileSystemView::new(root1, label1.clone(), permissions.clone());
    let view2 = FileSystemView::new(root2, label2.clone(), permissions.clone());
    let views = create_root(vec![view1, view2]);

    let mut root = FileSystemViewRoot::new(Some(views));
    root.change_working_directory(format!("/{}", label1.clone()));

    let listing = root.list_dir("..").map_err(|e| println!("{}", e)).unwrap();

    assert_eq!(3, listing.len());
    assert_eq!(
      HashSet::<&EntryData>::from_iter(listing.iter()).len(),
      listing.len()
    );
    validate_listing(&listing, 3, 2, 0, 2);
  }

  #[test]
  fn list_dir_view_parent_test() {
    let permissions = HashSet::from([UserPermission::READ, UserPermission::LIST]);

    let mut root1 = std::env::current_dir().unwrap();
    root1.push("test_files");
    let label1 = "test_files";
    let view1 = FileSystemView::new(root1, label1.clone(), permissions.clone());
    let views = create_root(vec![view1]);

    let mut root = FileSystemViewRoot::new(Some(views));
    root.change_working_directory(format!("/{}/subfolder", label1.clone()));

    let listing = root.list_dir("..").unwrap();

    validate_listing(&listing, 5, permissions.len(), 3, 1);
  }

  pub(crate) fn create_root(views: Vec<FileSystemView>) -> BTreeMap<String, FileSystemView> {
    views.into_iter().map(|v| (v.label.clone(), v)).collect()
  }
}
