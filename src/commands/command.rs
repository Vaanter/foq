//! The command and its argument.

use std::str::FromStr;
use tracing::{info, trace};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::commands::commands::Commands;
use crate::commands::r#impl::cdup::cdup;
use crate::commands::r#impl::cwd::cwd;
use crate::commands::r#impl::dele::dele;
use crate::commands::r#impl::feat::feat;
use crate::commands::r#impl::list::list;
use crate::commands::r#impl::mkd::mkd;
use crate::commands::r#impl::mlsd::mlsd;
use crate::commands::r#impl::nlst::nlst;
use crate::commands::r#impl::noop::noop;
use crate::commands::r#impl::pass::pass;
use crate::commands::r#impl::pasv::pasv;
use crate::commands::r#impl::pwd::pwd;
use crate::commands::r#impl::r#type::r#type;
use crate::commands::r#impl::rest::rest;
use crate::commands::r#impl::retr::retr;
use crate::commands::r#impl::rmd::rmd;
use crate::commands::r#impl::rmda::rmda;
use crate::commands::r#impl::stor::stor;
use crate::commands::r#impl::syst::syst;
use crate::commands::r#impl::user::user;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;

#[derive(Clone, Debug, PartialEq, Zeroize, ZeroizeOnDrop)]
pub(crate) struct Command {
  #[zeroize(skip)]
  pub(crate) command: Commands,
  pub(crate) argument: String,
}

impl Command {
  pub(crate) fn new(command: Commands, argument: impl Into<String>) -> Self {
    Command {
      command,
      argument: argument.into(),
    }
  }
}

impl Command {
  pub async fn execute(
    &self,
    command_processor: &mut CommandProcessor,
    reply_sender: &mut impl ReplySend,
  ) {
    match self.command {
      Commands::Cdup => cdup(self, command_processor, reply_sender).await,
      Commands::Cwd => cwd(self, command_processor, reply_sender).await,
      Commands::Dele => dele(self, command_processor, reply_sender).await,
      Commands::Feat => feat(self, reply_sender).await,
      Commands::List => list(self, command_processor, reply_sender).await,
      Commands::Mkd => mkd(self, command_processor, reply_sender).await,
      Commands::Nlst => nlst(self, command_processor, reply_sender).await,
      Commands::Mlsd => mlsd(self, command_processor, reply_sender).await,
      Commands::Noop => noop(self, reply_sender).await,
      Commands::Pass => pass(self, command_processor, reply_sender).await,
      Commands::Pasv => pasv(self, command_processor, reply_sender).await,
      Commands::Pwd => pwd(self, command_processor, reply_sender).await,
      Commands::Rest => rest(self, command_processor, reply_sender).await,
      Commands::Retr => retr(self, command_processor, reply_sender).await,
      Commands::Rmd => rmd(self, command_processor, reply_sender).await,
      Commands::Rmda => rmda(self, command_processor, reply_sender).await,
      Commands::Stor => stor(self, command_processor, reply_sender).await,
      Commands::Syst => syst(self, reply_sender).await,
      Commands::Type => r#type(self, command_processor, reply_sender).await,
      Commands::User => user(self, command_processor, reply_sender).await,
      _ => {
        info!(
          "Couldn't execute command, not implemented! Command: {:?}",
          self.command
        );
        reply_sender
          .send_control_message(Reply::new(
            ReplyCode::CommandNotImplemented,
            "Command not implemented!",
          ))
          .await
      }
    }
  }
}

impl FromStr for Command {
  type Err = anyhow::Error;

  #[tracing::instrument(skip(message))]
  fn from_str(message: &str) -> Result<Self, Self::Err> {
    trace!("Parsing message to command.");
    let message_trimmed = message.trim_end_matches(|c| c == '\n' || c == '\r');
    let split = message_trimmed
      .split_once(' ')
      .unwrap_or((message_trimmed, ""));
    let command = split.0;
    let argument = split.1;
    let command = Command::new(command.parse()?, argument);
    trace!("Command parsed: {:?}", command);
    Ok(command)
  }
}

#[cfg(test)]
mod tests {
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use std::str::FromStr;

  #[test]
  fn mlsd_test() {
    let parsed: Result<Command, anyhow::Error> = Command::from_str("mlsd test");
    assert!(parsed.is_ok());
    assert_eq!(Commands::Mlsd, parsed.as_ref().unwrap().command);
    assert_eq!("test", parsed.as_ref().unwrap().argument);
  }

  #[test]
  fn noop_test() {
    let parsed: Result<Command, anyhow::Error> = Command::from_str("noop");
    assert!(parsed.is_ok());
    assert_eq!(Commands::Noop, parsed.as_ref().unwrap().command);
    assert!(parsed.as_ref().unwrap().argument.is_empty());
  }

  #[test]
  fn user_test() {
    let parsed: Result<Command, anyhow::Error> = Command::from_str("user test");
    assert!(parsed.is_ok());
    assert_eq!(Commands::User, parsed.as_ref().unwrap().command);
    assert_eq!("test", parsed.as_ref().unwrap().argument);
  }
}
