//! Processes commands from client.

use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info};

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::commands::r#impl::cdup::Cdup;
use crate::commands::r#impl::cwd::Cwd;
use crate::commands::r#impl::feat::Feat;
use crate::commands::r#impl::mlsd::Mlsd;
use crate::commands::r#impl::noop::Noop;
use crate::commands::r#impl::pass::Pass;
use crate::commands::r#impl::pasv::Pasv;
use crate::commands::r#impl::pwd::Pwd;
use crate::commands::r#impl::r#type::Type;
use crate::commands::r#impl::retr::Retr;
use crate::commands::r#impl::stor::Stor;
use crate::commands::r#impl::syst::Syst;
use crate::commands::r#impl::user::User;
use crate::data_channels::data_channel_wrapper::DataChannelWrapper;
use crate::handlers::reply_sender::ReplySend;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::session::session_properties::SessionProperties;

#[derive(Clone)]
pub(crate) struct CommandProcessor {
  pub(crate) session_properties: Arc<RwLock<SessionProperties>>,
  pub(crate) data_wrapper: Arc<Mutex<dyn DataChannelWrapper>>,
}

impl CommandProcessor {
  /// Constructs new processor.
  /// 
  /// Holds session properties and data wrapper which can be used in commands.
  pub(crate) fn new(
    session_properties: Arc<RwLock<SessionProperties>>,
    data_wrapper: Arc<Mutex<dyn DataChannelWrapper>>,
  ) -> Self {
    CommandProcessor {
      session_properties,
      data_wrapper,
    }
  }

  /// Parses users message into command and then executes it.
  /// 
  /// The commands is first parsed. If parsing fails a reply is sent and this returns. If parsing 
  /// succeeds and the command is implemented, then it is executed. If it's not implemented then 
  /// a reply is sent stating such.
  /// 
  #[tracing::instrument(skip_all)]
  pub(crate) async fn evaluate(&mut self, message: String, reply_sender: &mut impl ReplySend) {
    debug!("Evaluating command");
    let command: Command = match message.trim().parse() {
      Ok(c) => c,
      Err(e) => {
        info!("Failed to parse command! Message: {message}. Error: {e}");
        if !message.trim().is_empty() {
          reply_sender
            .send_control_message(Reply::new(
              ReplyCode::SyntaxErrorCommandUnrecognized,
              "Command not parseable!",
            ))
            .await;
        }
        return;
      }
    };

    match command.command {
      Commands::CDUP => Cdup::execute(self, &command, reply_sender).await,
      Commands::CWD => Cwd::execute(self, &command, reply_sender).await,
      Commands::FEAT => Feat::execute(self, &command, reply_sender).await,
      Commands::MLSD => Mlsd::execute(self, &command, reply_sender).await,
      Commands::NOOP => Noop::execute(self, &command, reply_sender).await,
      Commands::PASS => Pass::execute(self, &command, reply_sender).await,
      Commands::PASV => Pasv::execute(self, &command, reply_sender).await,
      Commands::PWD => Pwd::execute(self, &command, reply_sender).await,
      Commands::RETR => Retr::execute(self, &command, reply_sender).await,
      Commands::STOR => Stor::execute(self, &command, reply_sender).await,
      Commands::SYST => Syst::execute(self, &command, reply_sender).await,
      Commands::TYPE => Type::execute(self, &command, reply_sender).await,
      Commands::USER => User::execute(self, &command, reply_sender).await,
      _ => {
        reply_sender
          .send_control_message(Reply::new(
            ReplyCode::CommandNotImplemented,
            "Command not implemented!",
          ))
          .await
      }
    };
  }
}
