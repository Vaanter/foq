use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;

use argon2::{Argon2, PasswordHash, PasswordVerifier};
use tokio::net::TcpListener;

use crate::auth::auth_error::AuthError;
use crate::auth::login_form::LoginForm;
use crate::auth::user_data::UserData;
use crate::auth::user_permission::UserPermission;
use crate::global_context::DB_LAZY;
// src/bin/server.rs
use crate::io::file_system_view::FileSystemView;

/// NOTE: this certificate is to be used for demonstration purposes only!
pub static CERT_PEM: &str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/certs/server-cert.pem"
));
/// NOTE: this certificate is to be used for demonstration purposes only!
pub static KEY_PEM: &str =
  include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/certs/server-key.pem"));

pub(crate) async fn get_port() {
  let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
  let listener = TcpListener::bind(addr).await;
  println!("Port: {}", listener.unwrap().local_addr().unwrap().port());
}

pub(crate) async fn sqlx_test() {
  let mut pool = DB_LAZY.acquire().await.unwrap();

  let argon = Argon2::default();
  let mut login_form = LoginForm::default();
  let username = login_form.username.insert("user1".to_string()).as_str();
  let password = login_form.password.insert("user1".to_string()).as_str();
  let query = sqlx::query!(
    "SELECT user_id, username, password FROM users WHERE username = $1",
    username
  )
  .fetch_one(&mut pool)
  .await
  .map_err(|e| AuthError::BackendError)
  .unwrap();
  let parsed_hash = PasswordHash::new(&query.password).unwrap();
  if argon
    .verify_password(password.as_bytes(), &parsed_hash)
    .is_err()
  {
    panic!("ERROR");
  }

  println!("{:#?}", query);
  let views = sqlx::query!(
    "SELECT root, label, permissions FROM views WHERE user_id = $1",
    query.user_id
  )
  .fetch_all(&mut pool)
  .await
  .map_err(|e| AuthError::BackendError)
  .unwrap();
  println!("{:#?}", views);

  let mut user_data = UserData::new(username.to_string(), parsed_hash.to_string());

  for view in views.iter() {
    let permissions = view
      .permissions
      .split(";")
      .map(|p| {
        println!("{}", p);
        UserPermission::from_str(p).unwrap()
      })
      .collect();
    let view = FileSystemView::new(PathBuf::from(&view.root), &view.label, permissions);
    user_data.add_view(view);
  }
  println!("{:#?}", user_data);
}
