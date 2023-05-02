use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use s2n_quic::stream::BidirectionalStream;
use s2n_quic::Connection;
use tokio::io::{AsyncBufReadExt, BufReader, ReadHalf};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use crate::handlers::connection_handler::ConnectionHandler;
use crate::handlers::quic_only_data_channel_wrapper::QuicOnlyDataChannelWrapper;
use crate::handlers::reply_sender::{ReplySend, ReplySender};
use crate::io::command_processor::CommandProcessor;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;
use crate::io::session_properties::SessionProperties;

pub(crate) struct QuicOnlyConnectionHandler {
  pub(crate) connection: Arc<Mutex<Connection>>,
  data_channel_wrapper: Arc<Mutex<QuicOnlyDataChannelWrapper>>,
  command_processor: Arc<Mutex<CommandProcessor>>,
  control_channel: Option<BufReader<ReadHalf<BidirectionalStream>>>,
  reply_loop: Option<JoinHandle<()>>,
  reply_sender: Option<ReplySender<BidirectionalStream>>,
  session_properties: Arc<RwLock<SessionProperties>>,
}

impl QuicOnlyConnectionHandler {
  pub(crate) fn new(connection: Connection) -> Self {
    let addr = connection.local_addr().unwrap();
    let connection = Arc::new(Mutex::new(connection));
    let wrapper = Arc::new(Mutex::new(QuicOnlyDataChannelWrapper::new(
      addr,
      connection.clone(),
    )));

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let command_processor = Arc::new(Mutex::new(CommandProcessor::new(
      session_properties.clone(),
      wrapper.clone(),
    )));

    QuicOnlyConnectionHandler {
      connection,
      data_channel_wrapper: wrapper,
      command_processor,
      control_channel: None,
      reply_loop: None,
      reply_sender: None,
      session_properties,
    }
  }

  pub(crate) fn get_command_processor(&self) -> Arc<Mutex<CommandProcessor>> {
    self.command_processor.clone()
  }

  async fn await_command(&mut self) -> Option<Reply> {
    let mut buf = String::new();
    println!("Server reading command!");
    let cc = self
      .control_channel
      .as_mut()
      .expect("Control channel must be open to receive commands!");
    let bytes = match cc.read_line(&mut buf).await {
      Ok(len) => {
        println!("Server command read!");
        len
      }
      Err(e) => {
        eprintln!("Failed to read command! {}", e);
        0
      }
    };
    if bytes > 0usize {
      let session = self.command_processor.clone();
      let reply_sender = self.reply_sender.as_mut().unwrap();
      session.lock().await.evaluate(buf, reply_sender).await;
      return None;
    }
    None
  }

  async fn create_control_channel(&mut self) -> Result<(), anyhow::Error> {
    let conn = self.connection.clone();

    return match conn.lock().await.open_bidirectional_stream().await {
      Ok(control_channel) => {
        let (reader, writer) = tokio::io::split(control_channel);
        let control_channel = BufReader::new(reader);
        let reply_sender = ReplySender::new(writer);
        let _ = self.control_channel.insert(control_channel);
        let _ = self.reply_sender.insert(reply_sender);
        Ok(())
      }
      Err(e) => Err(e.into()),
    };
  }
}

#[async_trait]
impl ConnectionHandler for QuicOnlyConnectionHandler {
  async fn handle(&mut self, token: CancellationToken) -> Result<(), anyhow::Error> {
    println!("Quic handler execute!");

    self.create_control_channel().await?;

    let hello = Reply::new(ReplyCode::ServiceReady, "Hello");
    let _ = &mut self.reply_sender.as_mut().unwrap().send_control_message(hello).await;

    loop {
      tokio::select! {
        biased;
        _ = token.cancelled() => {
          println!("Shutdown received!");
          if let Ok(conn) = timeout(Duration::from_secs(2), self.connection.clone().lock_owned()).await {
            conn.close(0u32.into())
          };
          break;
        },
        reply = self.await_command() => {
          if reply.is_some() {

          }
        }
      }
    }

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use s2n_quic::client::Connect;
  use s2n_quic::Client;
  use tokio::io::{AsyncBufReadExt, BufReader};
  use tokio::time::timeout;
  use tokio_util::sync::CancellationToken;

  use crate::handlers::connection_handler::ConnectionHandler;
  use crate::handlers::quic_only_connection_handler::QuicOnlyConnectionHandler;
  use crate::io::reply_code::ReplyCode;
  use crate::listeners::quic_only_listener::QuicOnlyListener;

  pub static CERT_PEM: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/certs/server-cert.pem"
  ));

  #[tokio::test]
  async fn server_hello_test() {
    let server_addr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");

    let mut listener = QuicOnlyListener::new(server_addr).unwrap();

    let addr = listener.server.local_addr().unwrap();
    println!("Server port is {}", addr.port());
    let token = CancellationToken::new();
    let ct = token.clone();
    let handler_fut = tokio::spawn(async move {
      let conn = listener.accept(ct.clone()).await.unwrap();
      let mut handler = QuicOnlyConnectionHandler::new(conn);

      handler
        .handle(ct)
        .await
        .expect("Handler should exit gracefully");
    });

    let client = Client::builder()
      .with_tls(CERT_PEM)
      .expect("Client requires valid TLS settings!")
      .with_io("0.0.0.0:0")
      .expect("Client requires valid I/O settings!")
      .start()
      .expect("Client must be able to start");

    let connect = Connect::new(addr).with_server_name("localhost");
    let mut connection = match client.connect(connect).await {
      Ok(conn) => conn,
      Err(e) => {
        panic!("Client failed to connect to the server! {}", e);
      }
    };

    let client_cc = match connection.accept_bidirectional_stream().await {
      Ok(Some(c)) => c,
      Ok(None) => {
        panic!("Connection closed when accepting control channel!")
      }
      Err(e) => {
        panic!("Client failed to accept control channel! {}", e);
      }
    };

    let (reader, writer) = client_cc.split();
    let mut client_reader = BufReader::new(reader);
    let mut buffer = String::new();
    match timeout(Duration::from_secs(3), client_reader.read_line(&mut buffer)).await {
      Ok(Ok(len)) => {
        println!("Received reply from server!: {}", buffer.trim());
        assert!(buffer
          .trim()
          .starts_with(&(ReplyCode::ServiceReady as u32).to_string()));
        assert!(buffer.trim().contains("Hello"));
        buffer.clear();
      }
      Ok(Err(e)) => {
        panic!("Failed to read reply! {}", e);
      }
      Err(e) => panic!("Timeout reading hello!"),
    }
    token.cancel();

    if let Err(e) = timeout(Duration::from_secs(3), handler_fut).await {
      panic!("Handler future failed to finish!");
    };
  }
}
