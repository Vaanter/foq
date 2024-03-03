use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;

use tokio::io::{AsyncBufReadExt, BufReader, ReadHalf};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::data_channels::data_channel_wrapper::DataChannelWrapper;
use crate::data_channels::standard_data_channel_wrapper::StandardDataChannelWrapper;
use crate::handlers::connection_handler::ConnectionHandler;
use crate::handlers::reply_sender::{ReplySend, ReplySender};
use crate::session::command_processor::CommandProcessor;
use crate::session::session_properties::SessionProperties;

/// Represents the networking part of clients session for TCP.
///
#[allow(unused)]
pub(crate) struct StandardConnectionHandler {
  data_channel_wrapper: Arc<StandardDataChannelWrapper>,
  command_processor: Arc<CommandProcessor>,
  control_channel: BufReader<ReadHalf<TcpStream>>,
  reply_sender: Arc<ReplySender<TcpStream>>,
  session_properties: Arc<RwLock<SessionProperties>>,
  running_commands: Vec<JoinHandle<()>>,
}

impl StandardConnectionHandler {
  /// Constructs a new handler for TCP connections.
  ///
  /// Initializes a new data channel wrapper from the connection. Also creates a new session for
  /// the client. [`SessionProperties`] and [`CommandProcessor`] are set up with default settings.
  /// The connection will be split into reader and writer halves. The writer will be used to
  /// construct [`ReplySender`], the reader will be used to read messages from client.
  ///
  pub(crate) fn new(stream: TcpStream) -> Self {
    let wrapper = Arc::new(StandardDataChannelWrapper::new(
      stream.local_addr().unwrap(),
    ));
    let stream_halves = tokio::io::split(stream);
    let control_channel = BufReader::new(stream_halves.0);
    let reply_sender = Arc::new(ReplySender::new(stream_halves.1));
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let command_processor = Arc::new(CommandProcessor::new(
      session_properties.clone(),
      wrapper.clone(),
    ));
    let running_commands = Vec::with_capacity(10);
    StandardConnectionHandler {
      data_channel_wrapper: wrapper,
      command_processor,
      control_channel,
      reply_sender,
      session_properties,
      running_commands,
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
    debug!("[TCP] Reading message from client.");
    let bytes = match reader.read_line(&mut buf).await {
      Ok(len) => {
        debug!("[TCP] Received message from client, length: {len}");
        len
      }
      Err(e) => {
        if e.kind() != ErrorKind::UnexpectedEof {
          error!("[TCP] Reading client message failed! Error: {e}");
        }
        0
      }
    };
    if bytes == 0 {
      anyhow::bail!("Connection closed!");
    }
    let command_processor = self.command_processor.clone();
    let task = command_processor.evaluate(buf, self.reply_sender.clone());
    self.running_commands.push(tokio::spawn(task));
    if self.running_commands.len() > 10 {
      self.running_commands.retain(|t| !t.is_finished());
    }
    debug!("Running commands: {}", self.running_commands.len());
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
    self.reply_sender.send_control_message(hello).await;
    loop {
      tokio::select! {
        biased;
        _ = token.cancelled() => {
          break;
        }
        result = self.await_command() => {
          if let Err(e) = result {
            info!("[TCP] Error awaiting command! {e}");
            break;
          }
        }
      }
    }
    self.cleanup().await;
    Ok(())
  }
}

impl StandardConnectionHandler {
  async fn cleanup(&mut self) {
    info!("[TCP] Shutdown received!");
    let mut commands_to_finish = std::mem::take(&mut self.running_commands);
    commands_to_finish.retain(|t| !t.is_finished());
    debug!("[TCP] Commands to finish: {:#?}", commands_to_finish);
    if timeout(Duration::from_secs(5), join_all(commands_to_finish))
      .await
      .is_err()
    {
      warn!("[TCP] Failed to finish processing running commands in time!");
    } else {
      debug!("[TCP] Finished processing running tasks");
    }
    if timeout(Duration::from_secs(5), self.reply_sender.close())
      .await
      .is_err()
    {
      warn!("[TCP] Failed to close command channel in time!");
    } else {
      debug!("[TCP] Closed command channel")
    }
    let data_channel_cleanup = async {
      debug!("[TCP] Aborting data channel");
      self.data_channel_wrapper.abort();
      debug!("[TCP] Closing data channel");
      self.data_channel_wrapper.close_data_stream().await;
    };
    if timeout(Duration::from_secs(5), data_channel_cleanup)
      .await
      .is_err()
    {
      warn!("[TCP] Failed to close data channel in time!")
    } else {
      debug!("[TCP] Closed data channel")
    }
  }
}

impl Drop for StandardConnectionHandler {
  fn drop(&mut self) {
    debug!("[TCP] aborting {} commands", self.running_commands.len());
    self.running_commands.iter().for_each(|t| t.abort());
  }
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use tokio::io::{AsyncBufReadExt, BufReader};
  use tokio::net::TcpStream;
  use tokio::time::timeout;
  use tokio_util::sync::CancellationToken;

  use crate::commands::reply_code::ReplyCode;
  use crate::handlers::connection_handler::ConnectionHandler;
  use crate::handlers::standard_connection_handler::StandardConnectionHandler;
  use crate::listeners::standard_listener::StandardListener;
  use crate::utils::test_utils::LOCALHOST;

  #[tokio::test]
  async fn server_hello_test() {
    let mut listener = StandardListener::new(LOCALHOST).await.unwrap();
    let addr = listener.listener.local_addr().unwrap();
    let token = CancellationToken::new();
    let ct = token.clone();
    let handler_fut = tokio::spawn(async move {
      let (server_cc, _) = listener.accept(ct.clone()).await.unwrap();
      let mut handler = StandardConnectionHandler::new(server_cc);

      handler
        .handle(ct)
        .await
        .expect("Handler should exit gracefully");
    });

    let client_cc = timeout(Duration::from_secs(2), TcpStream::connect(addr))
      .await
      .unwrap()
      .unwrap();
    let (reader, _) = tokio::io::split(client_cc);
    let mut client_reader = BufReader::new(reader);
    let mut buffer = String::new();
    match timeout(Duration::from_secs(3), client_reader.read_line(&mut buffer)).await {
      Ok(Ok(len)) => {
        println!(
          "Received reply from server!: {}. Length: {}",
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

    if (timeout(Duration::from_secs(3), handler_fut).await).is_err() {
      panic!("Handler future failed to finish!");
    };
  }
}
