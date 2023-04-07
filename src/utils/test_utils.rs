use async_trait::async_trait;
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::Sender;

use crate::handlers::connection_handler::ConnectionHandler;
use crate::handlers::reply_sender::ReplySend;
use crate::io::reply::Reply;

pub(crate) struct TestHandler;

#[async_trait]
impl ConnectionHandler for TestHandler {
    async fn handle(&mut self, receiver: Receiver<()>) -> Result<(), anyhow::Error> {
        todo!()
    }
}

pub(crate) struct TestReplySender {
    tx: Sender<Reply>
}

impl TestReplySender {
    pub(crate) fn new(tx: Sender<Reply>) -> Self {
        TestReplySender {
            tx
        }
    }
}

#[async_trait]
impl ReplySend for TestReplySender {
    async fn send_control_message(&self, reply: Reply) {
        println!("TestReplySender: received reply: {}", reply.to_string().trim_end());
        self.tx.send(reply).await.unwrap();
    }
}
