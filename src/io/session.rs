use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::auth::user_data::UserData;
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
use crate::handlers::data_wrapper::DataChannelWrapper;
use crate::handlers::reply_sender::ReplySend;
use crate::io::data_type::DataType;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;
use crate::io::transfer_mode::TransferMode;

pub(crate) struct Session {
  pub(crate) cwd: PathBuf,
  pub(crate) mode: TransferMode,
  pub(crate) data_type: DataType,
  pub(crate) user_data: Option<UserData>,
  pub(crate) data_wrapper: Arc<Mutex<dyn DataChannelWrapper + Send + Sync>>,
}

impl Session {
  pub(crate) fn new_with_defaults(
    data_wrapper: Arc<Mutex<dyn DataChannelWrapper + Send + Sync>>,
  ) -> Self {
    let cwd = std::env::current_dir().unwrap_or_else(|e| {
      panic!("Directory where the executable is stored must be accessible! Error: {e}")
    });
    Session {
      cwd,
      mode: TransferMode::Block,
      data_type: DataType::BINARY,
      user_data: None,
      data_wrapper,
    }
  }

  pub(crate) async fn evaluate(&mut self, message: String, reply_sender: &mut impl ReplySend) {
    let command = match Command::parse(&message.trim()) {
      Ok(c) => c,
      Err(e) => {
        eprintln!("Failed to parse command: '{}', {}", &message, e);
        Noop::reply(
          Reply::new(
            ReplyCode::SyntaxErrorCommandUnrecognized,
            "Command not parseable!",
          ),
          reply_sender,
        )
        .await;
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

  pub(crate) fn is_logged_in(&self) -> bool {
    return self.user_data.is_some();
  }

  pub(crate) fn set_user(&mut self, user: UserData) {
    self.user_data = Some(user);
  }

  pub(crate) fn set_path(&mut self, new_path: PathBuf) -> bool {
    let accessible = self.is_logged_in()
      && self
        .user_data
        .as_ref()
        .expect("User should be logged in here!")
        .acl
        .iter()
        .any(|ac| new_path.starts_with(ac.0) && *ac.1);
    if accessible {
      self.cwd = new_path;
    }
    accessible
  }
}
