//! Processes commands from client.

use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{debug, trace};
use zeroize::Zeroize;

use crate::commands::command::Command;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::data_channels::data_channel_wrapper::DataChannelWrapper;
use crate::handlers::reply_sender::ReplySend;
use crate::session::session_properties::SessionProperties;

#[derive(Clone)]
pub(crate) struct CommandProcessor {
  pub(crate) session_properties: Arc<RwLock<SessionProperties>>,
  pub(crate) data_wrapper: Arc<dyn DataChannelWrapper>,
}

impl CommandProcessor {
  /// Constructs new processor.
  ///
  /// Holds session properties and data wrapper which can be used in commands.
  pub(crate) fn new(
    session_properties: Arc<RwLock<SessionProperties>>,
    data_wrapper: Arc<dyn DataChannelWrapper>,
  ) -> Self {
    CommandProcessor {
      session_properties,
      data_wrapper,
    }
  }

  /// Parses users message into command and then executes it.
  ///
  /// The commands are first parsed. If parsing fails a reply is sent and this returns. If parsing
  /// succeeds and the command is implemented, then it is executed. If it's not implemented then
  /// a reply is sent stating such.
  ///
  #[tracing::instrument(skip_all)]
  pub(crate) async fn evaluate<T: ReplySend + Send>(
    self: Arc<Self>,
    mut message: String,
    reply_sender: Arc<T>,
  ) {
    debug!("Evaluating command");
    let command = message.trim().parse::<Command>();
    message.zeroize();
    match command {
      Ok(command) => {
        trace!("Parsed command: {:#?}", command);
        command.execute(self, reply_sender).await;
      }
      Err(e) => {
        debug!("Failed to parse command! Message: {message}. Error: {e}");
        if !message.trim().is_empty() {
          reply_sender
            .send_control_message(Reply::new(
              ReplyCode::SyntaxErrorCommandUnrecognized,
              "Command not parseable!",
            ))
            .await;
        }
      }
    };
  }
}
