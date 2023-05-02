use std::io::Error;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::time::timeout;

use crate::auth::auth_error::AuthError;
use crate::auth::auth_provider::AuthProvider;
use crate::auth::data_source::DataSource;
use crate::auth::login_form::LoginForm;
use crate::auth::user_data::UserData;
use crate::handlers::reply_sender::ReplySend;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;

// pub(crate) struct TestHandler;
//
// #[async_trait]
// impl ConnectionHandler for TestHandler {
//     async fn handle(&mut self, receiver: Receiver<()>) -> Result<(), anyhow::Error> {
//         todo!()
//     }
// }

pub(crate) struct TestReplySender {
  tx: Sender<Reply>,
}

impl TestReplySender {
  pub(crate) fn new(tx: Sender<Reply>) -> Self {
    TestReplySender { tx }
  }
}

#[async_trait]
impl ReplySend for TestReplySender {
  async fn send_control_message(&self, reply: Reply) {
    println!(
      "TestReplySender: received reply: {}",
      reply.to_string().trim_end()
    );
    self.tx.send(reply).await.unwrap();
  }

  async fn close(&mut self) -> Result<(), Error> {
    Ok(())
  }
}

#[derive(Clone, Default)]
pub(crate) struct TestDataSource {
  user_data: Vec<UserData>,
}

#[allow(unused)]
impl TestDataSource {
  pub(crate) fn new() -> Self {
    Self::default()
  }

  pub(crate) fn new_with_users(users: Vec<UserData>) -> Self {
    TestDataSource { user_data: users }
  }
}

#[async_trait]
impl DataSource for TestDataSource {
  async fn authenticate(&self, login_form: &LoginForm) -> Result<UserData, AuthError> {
    eprintln!("Received: {:?}", login_form);
    let user = self
      .user_data
      .iter()
      .find(|&u| &u.username == login_form.username.as_ref().unwrap())
      .ok_or(AuthError::UserNotFoundError)?;

    return if &user.password == login_form.password.as_ref().unwrap() {
      Ok(user.clone())
    } else {
      Err(AuthError::UserNotFoundError)
    }
  }
}

pub(crate) fn create_test_auth_provider(users: Vec<UserData>) -> AuthProvider {
  let mut provider = AuthProvider::new();
  provider.add_data_source(Box::new(TestDataSource::new_with_users(users)));
  provider
}

pub(crate) async fn receive_and_verify_reply(
  time: u64,
  rx: &mut Receiver<Reply>,
  expected: ReplyCode,
  substring: Option<&str>,
) {
  match timeout(Duration::from_secs(time), rx.recv()).await {
    Ok(Some(result)) => {
      assert_eq!(expected, result.code);
      if substring.is_some() {
        assert!(result.to_string().contains(substring.unwrap()));
      }
    }
    Err(_) | Ok(None) => {
      panic!("Failed to receive reply!");
    }
  };
}
