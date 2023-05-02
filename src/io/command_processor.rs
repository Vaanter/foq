use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};
use tracing::{info, trace};

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::commands::r#impl::auth::Auth;
use crate::commands::r#impl::cdup::Cdup;
use crate::commands::r#impl::feat::Feat;
use crate::commands::r#impl::mlsd::Mlsd;
use crate::commands::r#impl::noop::Noop;
use crate::commands::r#impl::pass::Pass;
use crate::commands::r#impl::pasv::Pasv;
use crate::commands::r#impl::pwd::Pwd;
use crate::commands::r#impl::r#type::Type;
use crate::commands::r#impl::stor::Stor;
use crate::commands::r#impl::syst::Syst;
use crate::commands::r#impl::user::User;
use crate::handlers::data_channel_wrapper::DataChannelWrapper;
use crate::handlers::reply_sender::ReplySend;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;
use crate::io::session_properties::SessionProperties;

#[derive(Clone)]
pub(crate) struct CommandProcessor {
  pub(crate) session_properties: Arc<RwLock<SessionProperties>>,
  pub(crate) data_wrapper: Arc<Mutex<dyn DataChannelWrapper>>,
}

impl CommandProcessor {
  pub(crate) fn new(
    session_properties: Arc<RwLock<SessionProperties>>,
    data_wrapper: Arc<Mutex<dyn DataChannelWrapper>>,
  ) -> Self {
    CommandProcessor {
      session_properties,
      data_wrapper,
    }
  }

  #[tracing::instrument(skip(self, reply_sender))]
  pub(crate) async fn evaluate(&mut self, message: String, reply_sender: &mut impl ReplySend) {
    trace!("Evaluating command");
    let command = match Command::parse(&message.trim()) {
      Ok(c) => c,
      Err(e) => {
        Noop::reply(
          Reply::new(
            ReplyCode::SyntaxErrorCommandUnrecognized,
            "Command not parseable!",
          ),
          reply_sender,
        )
        .await;
        info!("Failed to parse command! Error: {e}");
        return;
      }
    };

    match command.command {
      Commands::AUTH => Auth::execute(self, &command, reply_sender).await,
      Commands::CDUP => Cdup::execute(self, &command, reply_sender).await,
      Commands::FEAT => Feat::execute(self, &command, reply_sender).await,
      Commands::MLSD => Mlsd::execute(self, &command, reply_sender).await,
      Commands::PASS => Pass::execute(self, &command, reply_sender).await,
      Commands::PASV => Pasv::execute(self, &command, reply_sender).await,
      Commands::PWD => Pwd::execute(self, &command, reply_sender).await,
      Commands::STOR => Stor::execute(self, &command, reply_sender).await,
      Commands::SYST => Syst::execute(self, &command, reply_sender).await,
      Commands::TYPE => Type::execute(self, &command, reply_sender).await,
      Commands::USER => User::execute(self, &command, reply_sender).await,
      _ => {
        Noop::reply(
          Reply::new(ReplyCode::CommandNotImplemented, "Command not implemented!"),
          reply_sender,
        )
        .await
      }
    };
  }
}
