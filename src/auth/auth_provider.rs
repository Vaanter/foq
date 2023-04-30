use crate::auth::data_source::DataSource;
use crate::auth::login_form::LoginForm;
use crate::auth::user_data::UserData;

#[derive(Clone, Default)]
pub(crate) struct AuthProvider {
  data_sources: Vec<Box<dyn DataSource>>,
}

impl AuthProvider {
  pub(crate) fn new() -> Self {
    Self::default()
  }

  pub(crate) async fn authenticate(&self, login_form: &LoginForm) -> Option<UserData> {
    for data_source in self.data_sources.iter() {
      if let Ok(ud) = data_source.authenticate(login_form).await {
        return Some(ud);
      }
    }
    None
  }

  pub(crate) fn add_data_source(&mut self, data_source: Box<dyn DataSource>) {
    self.data_sources.push(data_source);
  }
}

#[cfg(test)]
mod tests {
  use sqlx::SqlitePool;

  use crate::auth::auth_provider::AuthProvider;
  use crate::auth::login_form::LoginForm;
  use crate::auth::sqlite_data_source::SqliteDataSource;
  use crate::auth::sqlite_data_source::tests::setup_test_db;

  #[sqlx::test]
  async fn authenticate_test(pool: SqlitePool) {
    setup_test_db(&pool).await.unwrap();
    let mut provider = AuthProvider::new();
    provider.add_data_source(Box::new(SqliteDataSource::new(pool)));
    let mut form = LoginForm::default();
    let _ = form.username.insert("user1".to_string());
    let _ = form.password.insert("user1".to_string());
    assert!(provider.authenticate(&form).await.is_some());
  }

  #[sqlx::test]
  async fn authenticate_invalid_test(pool: SqlitePool) {
    setup_test_db(&pool).await.unwrap();
    let mut provider = AuthProvider::new();
    provider.add_data_source(Box::new(SqliteDataSource::new(pool)));
    let mut form = LoginForm::default();
    let _ = form.username.insert("user1".to_string());
    let _ = form.password.insert("INVALID".to_string());
    assert!(provider.authenticate(&form).await.is_none());
  }
}
