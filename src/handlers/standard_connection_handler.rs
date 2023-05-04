use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader, ReadHalf};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::handlers::connection_handler::ConnectionHandler;
use crate::handlers::reply_sender::{ReplySend, ReplySender};
use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
use crate::io::command_processor::CommandProcessor;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;
use crate::io::session_properties::SessionProperties;

#[allow(unused)]
pub(crate) struct StandardConnectionHandler {
  data_channel_wrapper: Arc<Mutex<StandardDataChannelWrapper>>,
  command_processor: Arc<Mutex<CommandProcessor>>,
  control_channel: BufReader<ReadHalf<TcpStream>>,
  reply_sender: ReplySender<TcpStream>,
  session_properties: Arc<RwLock<SessionProperties>>,
}

impl StandardConnectionHandler {
  pub(crate) fn new(stream: TcpStream) -> Self {
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(
      stream.local_addr().unwrap().clone(),
    )));
    let stream_halves = tokio::io::split(stream);
    let control_channel = BufReader::new(stream_halves.0);
    let reply_sender = ReplySender::new(stream_halves.1);
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let command_processor = Arc::new(Mutex::new(CommandProcessor::new(
      session_properties.clone(),
      wrapper.clone(),
    )));
    StandardConnectionHandler {
      data_channel_wrapper: wrapper,
      command_processor,
      control_channel,
      reply_sender,
      session_properties,
    }
  }

  #[tracing::instrument(skip(self))]
  pub(crate) async fn await_command(&mut self) -> Result<(), anyhow::Error> {
    let reader = &mut self.control_channel;
    let mut buf = String::new();
    debug!("[TCP] Reading message from client.");
    let bytes = match reader.read_line(&mut buf).await {
      Ok(len) => {
        debug!(
          "[TCP] Received message from client, length: {len}, content: {}",
          buf.trim()
        );
        len
      }
      Err(e) => {
        error!("[TCP] Reading client message failed! Error: {e}");
        0
      }
    };
    if bytes == 0 {
      anyhow::bail!("Connection closed!");
    }

    let session = self.command_processor.clone();
    session
      .lock()
      .await
      .evaluate(buf, &mut self.reply_sender)
      .await;
    Ok(())
  }
}

#[async_trait]
impl ConnectionHandler for StandardConnectionHandler {
  #[tracing::instrument(skip(self, token))]
  async fn handle(&mut self, token: CancellationToken) -> Result<(), anyhow::Error> {
    debug!("[TCP] Handler started.");

    let hello = Reply::new(ReplyCode::ServiceReady, "Hello");
    debug!("[TCP] Sending hello to client.");
    let _ = &mut self.reply_sender.send_control_message(hello).await;

    loop {
      tokio::select! {
        biased;
        _ = token.cancelled() => {
          info!("[TCP] Shutdown received!");
          let _ = timeout(Duration::from_secs(2), self.reply_sender.close()).await;
          break;
        }
        result = self.await_command() => {
          if let Err(e) = result {
            warn!("[TCP] Error awaiting command!");
            if let Err(_) = timeout(Duration::from_secs(2), self.reply_sender.close()).await {
              warn!("[TCP] Failed to clean up after connection shutdown!");
            };
            break;
          }
        }
      }
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use std::net::SocketAddr;
  use std::time::Duration;

  use tokio::io;
  use tokio::io::{AsyncBufReadExt, BufReader};
  use tokio::net::{TcpListener, TcpStream};
  use tokio::time::timeout;
  use tokio_util::sync::CancellationToken;

  use crate::handlers::connection_handler::ConnectionHandler;
  use crate::handlers::data_channel_wrapper::DataChannelWrapper;
  use crate::handlers::standard_connection_handler::StandardConnectionHandler;
  use crate::io::reply_code::ReplyCode;

  #[tokio::test]
  async fn smoke() {
    let ip: SocketAddr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");

    let listener = TcpListener::bind(ip).await;
    assert!(listener.is_ok());
    let listener = listener.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
      loop {
        let _ = listener.accept().await;
      }
    });

    let stream = TcpStream::connect(addr).await.unwrap();
    let handler = StandardConnectionHandler::new(stream);

    let (port_send, port_recv) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
      let result = handler
        .data_channel_wrapper
        .clone()
        .lock()
        .await
        .open_data_stream()
        .await;
      assert!(result.is_ok());
      port_send.send(result.unwrap())
    });

    let port_msg = timeout(Duration::from_secs(3), port_recv).await;

    let addr = match port_msg {
      Ok(Ok(addr)) => {
        println!("Address is {}", addr);
        addr
      }
      Ok(Err(e)) => {
        panic!("Failed to receive port: {}", e);
      }
      Err(e) => {
        panic!("Failed to receive port: {}", e);
      }
    };
    println!("Connecting to passive listener");
    let client_dc: io::Result<TcpStream> = TcpStream::connect(addr).await;
    if let Err(e) = client_dc.as_ref() {
      panic!("{}", e);
    }
    println!("Connection successful!");
  }

  #[tokio::test]
  async fn server_hello_test() {
    let ip: SocketAddr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");

    let listener = match TcpListener::bind(ip).await {
      Ok(l) => l,
      Err(e) => {
        panic!("Failed to create server listener! {}", e);
      }
    };
    let addr = listener.local_addr().unwrap();
    println!("Server port is {}", addr.port());
    let token = CancellationToken::new();
    let ct = token.clone();
    let handler_fut = tokio::spawn(async move {
      let (server_cc, _) = listener.accept().await.unwrap();
      let mut handler = StandardConnectionHandler::new(server_cc);

      handler
        .handle(ct)
        .await
        .expect("Handler should exit gracefully");
    });

    let mut client_cc = TcpStream::connect(addr).await.unwrap();

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
