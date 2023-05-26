//! An authentication data source backed by an SQLite database.

use std::path::PathBuf;
use std::str::FromStr;

use argon2::{Argon2, PasswordHash, PasswordVerifier};
use async_trait::async_trait;
use sqlx::SqlitePool;
use tracing::warn;

use crate::auth::auth_error::AuthError;
use crate::auth::data_source::DataSource;
use crate::auth::login_form::LoginForm;
use crate::auth::user_data::UserData;
use crate::auth::user_permission::UserPermission;
use crate::io::file_system_view::FileSystemView;

#[derive(Clone)]
pub(crate) struct SqliteDataSource {
  pool: SqlitePool,
}

impl SqliteDataSource {
  /// Constructs a new [`SqliteDataSource`] instance.
  pub(crate) fn new(pool: SqlitePool) -> Self {
    SqliteDataSource { pool }
  }
}

#[async_trait]
impl DataSource for SqliteDataSource {
  /// Attempts to authenticate a user.
  ///
  /// Queries the database for an entry with matching username. Afterwards the passwords are
  /// compared. If passwords match, then users [`FileSystemView`]s are loaded and the [`UserData`]
  /// entity is constructed and returned.
  ///
  /// # Arguments
  ///
  /// - `login_form`: A [`LoginForm`] that contains the username and password.
  ///
  /// # Returns
  ///
  /// A [`Result`] containing the [`UserData`] entity if successful, or an [`AuthError`] if an
  /// error occurs.
  ///
  /// # Errors
  ///
  /// This function can return the following [`AuthError`] variants:
  ///
  /// - [`AuthError::BackendError`]: If a database errors occurs.
  /// - [`AuthError::UserNotFoundError`]: If the username is not in database.
  /// - [`AuthError::InvalidCredentials`]: If the password is incorrect.
  /// - [`AuthError::PermissionParsingError`]: If permissions have incorrect format.
  ///
  async fn authenticate(&self, login_form: &LoginForm) -> Result<UserData, AuthError> {
    if login_form.username.is_none() || login_form.password.is_none() {
      return Err(AuthError::BackendError);
    }

    let argon = Argon2::default();
    let username = login_form.username.as_ref().unwrap();
    let user_info = match sqlx::query!(
      "SELECT user_id, username, password FROM users WHERE username = $1",
      username
    )
    .fetch_optional(&self.pool)
    .await
    .map_err(|_| AuthError::BackendError)?
    {
      Some(r) => r,
      None => return Err(AuthError::UserNotFoundError),
    };
    let password_hash = user_info.password;
    let parsed_hash = PasswordHash::new(&password_hash).unwrap();
    if argon
      .verify_password(
        login_form.password.as_ref().unwrap().as_bytes(),
        &parsed_hash,
      )
      .is_err()
    {
      return Err(AuthError::InvalidCredentials);
    }

    let views = sqlx::query!(
      "SELECT root, label, permissions FROM views WHERE user_id = $1",
      user_info.user_id
    )
    .fetch_all(&self.pool)
    .await
    .map_err(|_| AuthError::BackendError)?;

    let mut user_data = UserData::new(username.to_string(), parsed_hash.to_string());

    for view in views.iter() {
      let permissions = Result::from_iter(
        view
          .permissions
          .trim()
          .split(";")
          .filter(|&p| !p.is_empty())
          .map(|p| UserPermission::from_str(p).map_err(|_| AuthError::PermissionParsingError)),
      )?;
      match FileSystemView::new_option(PathBuf::from(&view.root), &view.label, permissions) {
        Ok(v) => user_data.add_view(v),
        Err(_) => warn!("Failed to load view, the path may not exist! View: {:?}", view)
      }
    }

    Ok(user_data)
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use sqlx::SqlitePool;

  use crate::auth::auth_error::AuthError;
  use crate::auth::data_source::DataSource;
  use crate::auth::login_form::LoginForm;
  use crate::auth::sqlite_data_source::SqliteDataSource;

  pub(crate) async fn setup_test_db(pool: &SqlitePool) -> sqlx::Result<()> {
    sqlx::query_file!("sql/scheme.sql").execute(pool).await?;
    sqlx::query_file!("sql/data.sql").execute(pool).await?;
    Ok(())
  }

  #[sqlx::test]
  async fn login_test(pool: SqlitePool) -> sqlx::Result<()> {
    setup_test_db(&pool).await?;
    let data_source = SqliteDataSource::new(pool);

    let mut form = LoginForm::default();
    let _ = form.username.insert("user1".to_string());
    let _ = form.password.insert("user1".to_string());

    let result = data_source
      .authenticate(&form)
      .await
      .expect("Authenticate should succeed!");

    assert!(!result.file_system_views.is_empty());

    Ok(())
  }

  #[sqlx::test]
  async fn login_invalid_test(pool: SqlitePool) -> sqlx::Result<()> {
    setup_test_db(&pool).await?;
    let data_source = SqliteDataSource::new(pool);

    let mut form = LoginForm::default();
    let _ = form.username.insert("NONEXISTENT".to_string());
    let _ = form.password.insert("NONEXISTENT".to_string());

    let result = data_source.authenticate(&form).await;
    let Err(AuthError::UserNotFoundError) = result else {
      panic!("Expected UserNotFound error!");
    };

    Ok(())
  }

  #[sqlx::test]
  async fn login_invalid_password_test(pool: SqlitePool) -> sqlx::Result<()> {
    setup_test_db(&pool).await?;
    let data_source = SqliteDataSource::new(pool);

    let mut form = LoginForm::default();
    let _ = form.username.insert("user1".to_string());
    let _ = form.password.insert("NONEXISTENT".to_string());

    let result = data_source.authenticate(&form).await;
    let Err(AuthError::InvalidCredentials) = result else {
      panic!("Expected UserNotFound error!");
    };

    Ok(())
  }

  #[sqlx::test]
  async fn login_corrupted_permissions_test(pool: SqlitePool) -> sqlx::Result<()> {
    setup_test_db(&pool).await?;
    let data_source = SqliteDataSource::new(pool);

    let mut form = LoginForm::default();
    let _ = form.username.insert("user3".to_string());
    let _ = form.password.insert("user3".to_string());

    let result = data_source.authenticate(&form).await;
    let Err(AuthError::PermissionParsingError) = result else {
      panic!("Expected UserNotFound error!");
    };

    Ok(())
  }
}
