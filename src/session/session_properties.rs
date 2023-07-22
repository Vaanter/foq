//! Contains properties used throughout a session, such as username, datatype file system views 
//! and other.

use crate::auth::auth_provider::AuthProvider;
use crate::auth::login_form::LoginForm;
use crate::io::file_system_view_root::FileSystemViewRoot;
use crate::session::data_type::DataType;
use crate::session::transfer_mode::TransferMode;

/// Currently implemented properties.
#[allow(unused)]
#[derive(Debug, Default)]
pub(crate) struct SessionProperties {
  pub(crate) username: Option<String>,
  pub(crate) file_system_view_root: FileSystemViewRoot,
  pub(crate) transfer_mode: TransferMode,
  pub(crate) data_type: DataType,
  pub(crate) login_form: LoginForm,
  pub(crate) offset: u64,
}

impl SessionProperties {
  /// Constructs new session properties from defaults.
  pub(crate) fn new() -> Self {
    SessionProperties::default()
  }

  /// Return true if a user is logged in.
  pub(crate) fn is_logged_in(&self) -> bool {
    self.username.is_some()
  }

  /// Attempts to login the user.
  ///
  /// Passes the credentials from client to [`AuthProvider`]. If an authenticated user entity is
  /// returned, then the entity is used to set the username and [`FileSystemViewRoot`] is setup.
  ///
  /// Returns **true** if authentication succeeds, **false** otherwise.
  pub(crate) async fn login(
    &mut self,
    auth_provider: &AuthProvider,
    login_form: LoginForm,
  ) -> bool {
    let user_data = match auth_provider.authenticate(login_form).await {
      Some(data) => data,
      None => return false,
    };
    self.username.replace(user_data.username);
    self
      .file_system_view_root
      .set_views(user_data.file_system_views);
    true
  }
}
