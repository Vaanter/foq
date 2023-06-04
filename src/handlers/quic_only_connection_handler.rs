use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use s2n_quic::stream::BidirectionalStream;
use s2n_quic::Connection;
use tokio::io::{AsyncBufReadExt, BufReader, ReadHalf};
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::handlers::connection_handler::ConnectionHandler;
use crate::data_channels::quic_only_data_channel_wrapper::QuicOnlyDataChannelWrapper;
use crate::handlers::reply_sender::{ReplySend, ReplySender};
use crate::session::command_processor::CommandProcessor;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::data_channels::data_channel_wrapper::DataChannelWrapper;
use crate::session::session_properties::SessionProperties;

/// Represents the networking part of clients session for QUIC.
///
#[allow(unused)]
pub(crate) struct QuicOnlyConnectionHandler {
  connection: Arc<Mutex<Connection>>,
  data_channel_wrapper: Arc<Mutex<QuicOnlyDataChannelWrapper>>,
  command_processor: Arc<Mutex<CommandProcessor>>,
  control_channel: Option<BufReader<ReadHalf<BidirectionalStream>>>,
  reply_sender: Option<ReplySender<BidirectionalStream>>,
  session_properties: Arc<RwLock<SessionProperties>>,
}

impl QuicOnlyConnectionHandler {
  /// Constructs a new handler for QUIC connections.
  ///
  /// Initializes a new data channel wrapper from the connection. Also creates a new session for
  /// the client. [`SessionProperties`] and [`CommandProcessor`] are setup with default settings.
  ///
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
      reply_sender: None,
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
    let cc = self
      .control_channel
      .as_mut()
      .expect("Control channel must be open to receive commands!");
    let mut buf = String::new();
    debug!("Reading message from client.");
    let bytes = match cc.read_line(&mut buf).await {
      Ok(len) => {
        debug!(
          "[QUIC] Received message from client, length: {len}, content: {}",
          buf.trim()
        );
        len
      }
      Err(e) => {
        if e.kind() != ErrorKind::UnexpectedEof {
          error!("[QUIC] Reading client message failed! Error: {e}");
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
      .evaluate(buf, self.reply_sender.as_mut().unwrap())
      .await;
    Ok(())
  }

  /// Initiates a bidirectional stream, that will function as the control channel.
  ///
  /// Opens a new bidirectional stream. This stream will be split into reader and writer halves.
  /// The writer will be used to construct [`ReplySender`], the reader will be used to read
  /// messages from client.
  ///
  /// This will return an [`anyhow::Error`] if creating the stream fails.
  ///
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
    debug!("[QUIC] Handler started.");

    self.create_control_channel().await?;

    let hello = Reply::new(ReplyCode::ServiceReady, "Hello");
    debug!("[QUIC] Sending hello to client.");
    let _ = &mut self
      .reply_sender
      .as_mut()
      .unwrap()
      .send_control_message(hello)
      .await;

    loop {
      tokio::select! {
        biased;
        _ = token.cancelled() => {
          info!("[QUIC] Shutdown received!");
          let _ = timeout(Duration::from_secs(2), self.reply_sender.as_mut().unwrap().close());
          let _ = timeout(Duration::from_secs(2), self.data_channel_wrapper.lock().await.close_data_stream());
          break;
        }
        result = self.await_command() => {
          if let Err(e) = result {
            warn!("[QUIC] Error awaiting command! {e}.");
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

  use s2n_quic::client::Connect;
  use s2n_quic::provider::tls::default::Client as TlsClient;
  use s2n_quic::Client;
  use tokio::io::BufReader;
  use tokio::time::timeout;
  use tokio_util::sync::CancellationToken;

  use crate::commands::reply_code::ReplyCode;
  use crate::utils::test_utils::{
    create_tls_client_config, receive_and_verify_reply_from_buf, run_quic_listener, LOCALHOST,
  };

  #[tokio::test]
  async fn server_hello_test() {
    let token = CancellationToken::new();
    let (handler_fut, addr) = run_quic_listener(token.clone(), LOCALHOST).await;

    let tls_client = TlsClient::new(create_tls_client_config("ftpoq-1"));

    let client = Client::builder()
      .with_tls(tls_client)
      .expect("Client requires valid TLS settings!")
      .with_io(LOCALHOST)
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
        panic!("Connection closed when accepting control channel!");
      }
      Err(e) => {
        panic!("Client failed to accept control channel! {}", e);
      }
    };

    let (reader, _) = client_cc.split();
    let mut client_reader = BufReader::new(reader);
    receive_and_verify_reply_from_buf(
      2,
      &mut client_reader,
      ReplyCode::ServiceReady,
      Some("Hello"),
    )
    .await;
    token.cancel();

    if let Err(_) = timeout(Duration::from_secs(3), handler_fut).await {
      panic!("Handler future failed to finish!");
    };
  }

  #[tokio::test]
  async fn server_hello_quinn_test() {
    let token = CancellationToken::new();
    let (handler_fut, addr) = run_quic_listener(token.clone(), LOCALHOST).await;

    let client_config = create_tls_client_config("ftpoq-1");

    let mut quinn_client = quinn::Endpoint::client(LOCALHOST).unwrap();

    quinn_client.set_default_client_config(quinn::ClientConfig::new(Arc::new(client_config)));

    let connection = match quinn_client.connect(addr, "localhost").unwrap().await {
      Ok(conn) => conn,
      Err(e) => {
        panic!("Client failed to connect to the server! {}", e);
      }
    };

    let (_, reader) = match connection.accept_bi().await {
      Ok(c) => c,
      Err(e) => {
        panic!("Client failed to accept control channel! {}", e);
      }
    };

    let mut client_reader = BufReader::new(reader);
    receive_and_verify_reply_from_buf(
      2,
      &mut client_reader,
      ReplyCode::ServiceReady,
      Some("Hello"),
    )
    .await;
    token.cancel();

    if let Err(_) = timeout(Duration::from_secs(3), handler_fut).await {
      panic!("Handler future failed to finish!");
    };
  }
}
