use tracing::trace;

use crate::commands::commands::Commands;

#[derive(Clone, Debug, PartialEq)]
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

  #[tracing::instrument(skip(message))]
  pub(crate) fn parse(message: &str) -> Result<Self, anyhow::Error> {
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
