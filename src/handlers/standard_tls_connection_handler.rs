use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader, ReadHalf};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;
use tokio_rustls::server::TlsStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::handlers::connection_handler::ConnectionHandler;
use crate::handlers::reply_sender::{ReplySend, ReplySender};
use crate::data_channels::standard_data_channel_wrapper::StandardDataChannelWrapper;
use crate::session::command_processor::CommandProcessor;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::session::session_properties::SessionProperties;

/// Represents the networking part of clients session for TCP+TLS.
///
#[allow(unused)]
pub(crate) struct StandardTlsConnectionHandler {
  data_channel_wrapper: Arc<Mutex<StandardDataChannelWrapper>>,
  command_processor: Arc<Mutex<CommandProcessor>>,
  control_channel: BufReader<ReadHalf<TlsStream<TcpStream>>>,
  reply_sender: ReplySender<TlsStream<TcpStream>>,
  session_properties: Arc<RwLock<SessionProperties>>,
}

impl StandardTlsConnectionHandler {
  /// Constructs a new handler for TCP+TLS connections.
  ///
  /// Initializes a new data channel wrapper from the connection. Also creates a new session for
  /// the client. [`SessionProperties`] and [`CommandProcessor`] are setup with default settings.
  /// The connection will be split into reader and writer halves. The writer will be used to
  /// construct [`ReplySender`], the reader will be used to read messages from client.
  ///
  pub(crate) fn new(stream: TlsStream<TcpStream>) -> Self {
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(
      stream.get_ref().0.local_addr().unwrap().clone(),
    )));
    let stream_halves = tokio::io::split(stream);
    let control_channel = BufReader::new(stream_halves.0);
    let reply_sender = ReplySender::new(stream_halves.1);
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let command_processor = Arc::new(Mutex::new(CommandProcessor::new(
      session_properties.clone(),
      wrapper.clone(),
    )));
    StandardTlsConnectionHandler {
      data_channel_wrapper: wrapper,
      command_processor,
      control_channel,
      reply_sender,
      session_properties,
    }
  }

  /// Waits until the client sends a command or the connection closes.
  ///
  /// Reads data from the client until newline. If the connection closes, this returns an [`error`].
  /// Otherwise it will return [`Ok(())`].
  ///
  /// After reading clients message, it sent for evaluation to [`CommandProcessor`].
  ///
  /// [`error`]: anyhow::Error
  ///
  #[tracing::instrument(skip(self))]
  pub(crate) async fn await_command(&mut self) -> Result<(), anyhow::Error> {
    let reader = &mut self.control_channel;
    let mut buf = String::new();
    debug!("[TCP+TLS] Reading message from client.");
    let bytes = match reader.read_line(&mut buf).await {
      Ok(len) => {
        debug!(
          "[TCP+TLS] Received message from client, length: {len}, content: {}",
          buf.trim()
        );
        len
      }
      Err(e) => {
        if e.kind() != ErrorKind::UnexpectedEof {
          error!("[TCP+TLS] Reading client message failed! Error: {e}");
        }
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
impl ConnectionHandler for StandardTlsConnectionHandler {
  #[tracing::instrument(skip(self, token))]
  async fn handle(&mut self, token: CancellationToken) -> Result<(), anyhow::Error> {
    debug!("[TCP+TLS] Handler started.");

    let hello = Reply::new(ReplyCode::ServiceReady, "Hello");
    debug!("[TCP+TLS] Sending hello to client.");
    let _ = &mut self.reply_sender.send_control_message(hello).await;

    loop {
      tokio::select! {
        biased;
        _ = token.cancelled() => {
          info!("[TCP+TLS] Shutdown received!");
          let _ = timeout(Duration::from_secs(2), self.reply_sender.close()).await;
          break;
        }
        result = self.await_command() => {
          if let Err(e) = result {
            warn!("[TCP+TLS] Error awaiting command! {e}");
            if let Err(_) = timeout(Duration::from_secs(2), self.reply_sender.close()).await {
              warn!("[TCP+TLS] Failed to clean up after connection shutdown!");
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
  use std::sync::Arc;
  use std::time::Duration;

  use rustls::ServerName;
  use tokio::io::{AsyncBufReadExt, BufReader};
  use tokio::net::TcpStream;
  use tokio::time::timeout;
  use tokio_rustls::{TlsAcceptor, TlsConnector};
  use tokio_util::sync::CancellationToken;

  use crate::handlers::connection_handler::ConnectionHandler;
  use crate::handlers::standard_tls_connection_handler::StandardTlsConnectionHandler;
  use crate::commands::reply_code::ReplyCode;
  use crate::listeners::standard_listener::StandardListener;
  use crate::utils::test_utils::{create_test_client_config, create_test_server_config, LOCALHOST};

  #[tokio::test]
  async fn server_hello_test() {
    let token = CancellationToken::new();
    let ct = token.clone();
    let mut listener = StandardListener::new(LOCALHOST).await.unwrap();
    let addr = listener.listener.local_addr().unwrap();
    let server_acceptor = TlsAcceptor::from(Arc::new(create_test_server_config()));
    let handler_fut = tokio::spawn(async move {
      let (server_stream, _) = listener.accept(ct.clone()).await.unwrap();
      let server_cc = server_acceptor.accept(server_stream).await.unwrap();
      let mut handler = StandardTlsConnectionHandler::new(server_cc);

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

    let (reader, _) = tokio::io::split(client_cc);
    let mut client_reader = BufReader::new(reader);
    let mut buffer = String::new();
    match timeout(Duration::from_secs(3), client_reader.read_line(&mut buffer)).await {
      Ok(Ok(len)) => {
        println!(
          "Received reply from server!: {}. Length: {}.",
          buffer.trim(),
          len
        );
        assert!(buffer
          .trim()
          .starts_with(&(ReplyCode::ServiceReady as u32).to_string()));
        assert!(buffer.trim().contains("Hello"));
        buffer.clear();
      }
      Ok(Err(e)) => {
        panic!("Failed to read reply! {}", e);
      }
      Err(_) => panic!("Timeout reading hello!"),
    }
    token.cancel();

    if let Err(_) = timeout(Duration::from_secs(3), handler_fut).await {
      panic!("Handler future failed to finish!");
    };
  }
}
