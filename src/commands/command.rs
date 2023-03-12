use crate::commands::commands::Commands;
use anyhow::anyhow;

pub(crate) struct Command {
    pub(crate) command: Commands,
    pub(crate) argument: String,
}

impl Command {
    pub(crate) fn new(command: Commands, argument: impl Into<String>) -> Self {
        return Command { command, argument: argument.into() };
    }

    pub(crate) fn parse(message: &str) -> Result<Self, anyhow::Error> {
        let mut split = message.split(" ");
        let command = split.next().ok_or_else(|| anyhow!("Invalid command!"))?;
        let argument = split.next().unwrap_or("");
        Ok(Command {
            command: command.parse()?,
            argument: argument.to_owned(),
        })
    }
}
