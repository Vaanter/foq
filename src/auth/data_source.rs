use async_trait::async_trait;
use dyn_clone::DynClone;

use crate::auth::auth_error::AuthError;
use crate::auth::login_form::LoginForm;
use crate::auth::user_data::UserData;

#[async_trait]
pub(crate) trait DataSource: DynClone + Send + Sync {
  async fn authenticate(&self, login_form: &LoginForm) -> Result<UserData, AuthError>;
}

dyn_clone::clone_trait_object!(DataSource);
