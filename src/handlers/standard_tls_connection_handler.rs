use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rustls::ServerConfig;
use tokio::io::{AsyncBufReadExt, BufReader, ReadHalf};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;
use tokio_rustls::server::TlsStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::handlers::connection_handler::ConnectionHandler;
use crate::handlers::reply_sender::{ReplySend, ReplySender};
use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
use crate::io::command_processor::CommandProcessor;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;
use crate::io::session_properties::SessionProperties;

pub(crate) struct StandardConnectionHandler {
  data_channel_wrapper: Arc<Mutex<StandardDataChannelWrapper>>,
  command_processor: Arc<Mutex<CommandProcessor>>,
  control_channel: BufReader<ReadHalf<TlsStream<TcpStream>>>,
  reply_sender: ReplySender<TlsStream<TcpStream>>,
  session_properties: Arc<RwLock<SessionProperties>>,
}

impl StandardConnectionHandler {
  pub(crate) fn new(stream: TlsStream<TcpStream>, tls_config: ServerConfig) -> Self {
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(
      stream.get_ref().0.local_addr().unwrap().clone(),
      tls_config,
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

  pub(crate) fn get_command_processor(&self) -> Arc<Mutex<CommandProcessor>> {
    self.command_processor.clone()
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
  use std::sync::Arc;
  use std::time::Duration;

  use rustls::ServerName;
  use tokio::io::{AsyncBufReadExt, BufReader};
  use tokio::net::TcpStream;
  use tokio::time::timeout;
  use tokio_rustls::TlsConnector;
  use tokio_util::sync::CancellationToken;

  use crate::handlers::connection_handler::ConnectionHandler;
  use crate::handlers::standard_connection_handler::StandardConnectionHandler;
  use crate::io::reply_code::ReplyCode;
  use crate::listeners::standard_listener::StandardListener;
  use crate::utils::tls_utils::create_test_client_config;

  #[tokio::test]
  async fn server_hello_test() {
    let ip: SocketAddr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");

    let token = CancellationToken::new();
    let ct = token.clone();
    let mut listener = StandardListener::new(ip).await.unwrap();
    let addr = listener.listener.local_addr().unwrap();
    let handler_fut = tokio::spawn(async move {
      let (server_stream, _) = listener.accept(ct.clone()).await.unwrap();
      let server_cc = listener
        .get_acceptor_clone()
        .accept(server_stream)
        .await
        .unwrap();
      let mut handler = StandardConnectionHandler::new(server_cc);

      handler
        .handle(ct)
        .await
        .expect("Handler should exit gracefully");
    });

    let client_stream = timeout(Duration::from_secs(2), TcpStream::connect(addr))
      .await
      .unwrap()
      .unwrap();
    let client_tls_config = create_test_client_config();
    let connector = TlsConnector::from(Arc::new(client_tls_config));
    let domain = ServerName::try_from("localhost").unwrap();
    let client_cc = timeout(
      Duration::from_secs(2),
      connector.connect(domain, client_stream),
    )
    .await
    .unwrap()
    .unwrap();

    let (reader, writer) = tokio::io::split(client_cc);
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
