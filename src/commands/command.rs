use anyhow::anyhow;
use tracing::debug;

use crate::commands::commands::Commands;

pub(crate) struct Command {
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

  #[tracing::instrument]
  pub(crate) fn parse(message: &str) -> Result<Self, anyhow::Error> {
    debug!("Parsing message to command.");
    let mut split = message.split(" ");
    let command = split.next().ok_or_else(|| anyhow!("Invalid command!"))?;
    let argument = split.next().unwrap_or("");
    Ok(Command {
      command: command.parse()?,
      argument: argument.to_owned(),
    })
    debug!("Command parsed: {:?}", command);
  }
}

#[cfg(test)]
mod tests {
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;

  #[test]
  fn mlsd_test() {
    let parsed = Command::parse("mlsd test");
    assert!(parsed.is_ok());
    assert_eq!(Commands::MLSD, parsed.as_ref().unwrap().command);
    assert_eq!("test", parsed.as_ref().unwrap().argument);
  }

  #[test]
  fn noop_test() {
    let parsed = Command::parse("noop");
    assert!(parsed.is_ok());
    assert_eq!(Commands::NOOP, parsed.as_ref().unwrap().command);
    assert!(parsed.as_ref().unwrap().argument.is_empty());
  }

  #[test]
  fn user_test() {
    let parsed = Command::parse("user test");
    assert!(parsed.is_ok());
    assert_eq!(Commands::USER, parsed.as_ref().unwrap().command);
    assert_eq!("test", parsed.as_ref().unwrap().argument);
  }
}
