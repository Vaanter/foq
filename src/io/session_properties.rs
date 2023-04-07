use crate::auth::login_form::LoginForm;
use crate::auth::user_data::UserData;
use crate::io::data_type::DataType;
use crate::io::file_system_view_root::FileSystemViewRoot;
use crate::io::transfer_mode::TransferMode;

#[derive(Debug, Default)]
pub(crate) struct SessionProperties {
  pub(crate) username: Option<String>,
  pub(crate) file_system_view_root: FileSystemViewRoot,
  pub(crate) transfer_mode: TransferMode,
  pub(crate) data_type: DataType,
  pub(crate) login_form: LoginForm,
}

impl SessionProperties {
  pub(crate) fn new() -> Self {
    SessionProperties::default()
  }

  pub(crate) fn is_logged_in(&self) -> bool {
    self.username.is_some()
  }

  pub(crate) fn login(&mut self, user_data: UserData) {
    self.username.replace(user_data.username);
    self.file_system_view_root.set_views(user_data.file_system_views);
  }
}
