//! The command and its argument.

use std::str::FromStr;
use tracing::trace;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::commands::commands::Commands;

#[derive(Clone, Debug, PartialEq, Zeroize, ZeroizeOnDrop)]
pub(crate) struct Command {
  #[zeroize(skip)]
  pub(crate) command: Commands,
  pub(crate) argument: String,
}

impl Command {
  pub(crate) fn new(command: Commands, argument: impl Into<String>) -> Self {
    return Command {
      command,
      argument: argument.into(),
    };
  }
}

impl FromStr for Command {
  type Err = anyhow::Error;

  #[tracing::instrument(skip(message))]
  fn from_str(message: &str) -> Result<Self, Self::Err> {
    trace!("Parsing message to command.");
    let message_trimmed = message.trim_end_matches(|c| c == '\n' || c == '\r');
    let split = message_trimmed
      .split_once(" ")
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
  use std::str::FromStr;
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;

  #[test]
  fn mlsd_test() {
    let parsed: Result<Command, anyhow::Error> = Command::from_str("mlsd test");
    assert!(parsed.is_ok());
    assert_eq!(Commands::MLSD, parsed.as_ref().unwrap().command);
    assert_eq!("test", parsed.as_ref().unwrap().argument);
  }

  #[test]
  fn noop_test() {
    let parsed: Result<Command, anyhow::Error> = Command::from_str("noop");
    assert!(parsed.is_ok());
    assert_eq!(Commands::NOOP, parsed.as_ref().unwrap().command);
    assert!(parsed.as_ref().unwrap().argument.is_empty());
  }

  #[test]
  fn user_test() {
    let parsed: Result<Command, anyhow::Error> = Command::from_str("user test");
    assert!(parsed.is_ok());
    assert_eq!(Commands::USER, parsed.as_ref().unwrap().command);
    assert_eq!("test", parsed.as_ref().unwrap().argument);
  }
}
