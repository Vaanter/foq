use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::time::timeout;

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
