use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;
use quinn::{Connection, RecvStream, SendStream};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::data_channels::data_channel_wrapper::DataChannelWrapper;
use crate::data_channels::quic_quinn_data_channel_wrapper::QuicQuinnDataChannelWrapper;
use crate::handlers::connection_handler::ConnectionHandler;
use crate::handlers::reply_sender::{ReplySend, ReplySender};
use crate::session::command_processor::CommandProcessor;
use crate::session::session_properties::SessionProperties;

/// Represents the networking part of clients session for QUIC.
///
#[allow(unused)]
pub(crate) struct QuicQuinnConnectionHandler {
  connection: Arc<Mutex<Connection>>,
  data_channel_wrapper: Arc<QuicQuinnDataChannelWrapper>,
  command_processor: Arc<CommandProcessor>,
  control_channel: Option<BufReader<RecvStream>>,
  reply_sender: Option<Arc<ReplySender<SendStream>>>,
  session_properties: Arc<RwLock<SessionProperties>>,
  running_commands: Vec<JoinHandle<()>>,
}

impl QuicQuinnConnectionHandler {
  /// Constructs a new handler for QUIC connections using Quinn.
  ///
  /// Initializes a new data channel wrapper from the connection. Also creates a new session for
  /// the client. [`SessionProperties`] and [`CommandProcessor`] are set up with default settings.
  ///
  pub(crate) fn new(connection: Connection) -> Self {
    let addr = SocketAddr::new(connection.local_ip().unwrap(), 0);
    let connection = Arc::new(Mutex::new(connection));
    let wrapper = Arc::new(QuicQuinnDataChannelWrapper::new(addr, connection.clone()));

    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let command_processor = Arc::new(CommandProcessor::new(
      session_properties.clone(),
      wrapper.clone(),
    ));
    let running_commands = Vec::with_capacity(10);
    QuicQuinnConnectionHandler {
      connection,
      data_channel_wrapper: wrapper,
      command_processor,
      control_channel: None,
      reply_sender: None,
      session_properties,
      running_commands,
    }
  }

  /// Waits until the client sends a command or the connection closes.
  ///
  /// Reads data from the client until newline. If the connection closes, this returns an [`error`].
  /// Otherwise, it will return [`Ok(())`].
  ///
  /// After reading clients message, it sent for evaluation to [`CommandProcessor`].
  ///
  /// [`error`]: anyhow::Error
  ///
  #[tracing::instrument(skip(self))]
  pub(crate) async fn await_command(&mut self) -> Result<bool, anyhow::Error> {
    let cc = self
      .control_channel
      .as_mut()
      .expect("Control channel must be open to receive commands!");
    let mut buf = String::new();
    debug!("[QUINN] Reading message from client.");
    match cc.read_line(&mut buf).await {
      Ok(0) => {
        return Ok(false);
      }
      Ok(len) => {
        debug!("[QUINN] Received message from client, length: {len}");
      }
      Err(e) => {
        return Err(anyhow::Error::from(e));
      }
    };

    let command_processor = self.command_processor.clone();
    let task = command_processor.evaluate(buf, self.reply_sender.clone().unwrap());
    self.running_commands.push(tokio::spawn(task));
    self.running_commands.retain(|c| !c.is_finished());
    debug!("Running commands: {}", self.running_commands.len());
    Ok(true)
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

    match conn.lock_owned().await.open_bi().await {
      Ok((writer, reader)) => {
        let reply_sender = Arc::new(ReplySender::new(writer));
        self.control_channel.replace(BufReader::new(reader));
        self.reply_sender.replace(reply_sender);
        Ok(())
      }
      Err(e) => Err(e.into()),
    }
  }
}

#[async_trait]
impl ConnectionHandler for QuicQuinnConnectionHandler {
  #[tracing_attributes::instrument(skip_all, fields(_remote_addr))]
  async fn handle(&mut self, token: CancellationToken) -> Result<(), anyhow::Error> {
    debug!("[QUINN] Handler started.");

    self.create_control_channel().await?;

    let hello = Reply::new(ReplyCode::ServiceReady, "Hello");
    debug!("[QUINN] Sending hello to client.");
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
          info!("[QUINN] Shutdown received!");
          break;
        }
        result = self.await_command() => {
          match result {
            Ok(true) => {},
            Ok(false) => break,
            Err(e) => {
              warn!("[QUINN] Reading client message failed! Error: {e}!");
              break;
            }
          }
        }
      }
    }
    self.cleanup().await;
    Ok(())
  }
}

impl QuicQuinnConnectionHandler {
  async fn cleanup(&mut self) {
    info!("[QUINN] Shutdown received!");
    let commands_to_finish = join_all(std::mem::take(&mut self.running_commands));
    if timeout(Duration::from_secs(5), commands_to_finish)
      .await
      .is_err()
    {
      warn!("[QUINN] Failed to finish processing running commands in time!");
    }
    if timeout(
      Duration::from_secs(2),
      self.reply_sender.as_mut().unwrap().close(),
    )
    .await
    .is_err()
    {
      warn!("[QUINN] Failed to close command channel in time!");
    };
    let data_channel = self.data_channel_wrapper.clone();
    let data_channel_cleanup = async {
      data_channel.abort();
      data_channel.close_data_stream().await;
    };
    if timeout(Duration::from_secs(2), data_channel_cleanup)
      .await
      .is_err()
    {
      warn!("[QUINN] Failed to close data channel in time!")
    };
  }
}

impl Drop for QuicQuinnConnectionHandler {
  fn drop(&mut self) {
    debug!("[QUINN] aborting {} commands", self.running_commands.len());
    self.running_commands.iter().for_each(|c| c.abort());
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::utils::test_utils::*;
  use s2n_quic::client::Connect;

  #[tokio::test]
  async fn server_hello_quinn_test() {
    let token = CancellationToken::new();
    let (handler_fut, addr) = run_quinn_listener(token.clone());

    let tls_config = create_tls_client_config("ftpoq-1");
    let quinn_client = setup_quinn_client(tls_config);

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

    if timeout(Duration::from_secs(3), handler_fut).await.is_err() {
      panic!("Handler future failed to finish!");
    };
  }

  #[tokio::test]
  async fn server_hello_test() {
    let token = CancellationToken::new();
    let (handler_fut, addr) = run_quinn_listener(token.clone());

    let client = setup_s2n_client();

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

    if timeout(Duration::from_secs(3), handler_fut).await.is_err() {
      panic!("Handler future failed to finish!");
    };
  }
}
