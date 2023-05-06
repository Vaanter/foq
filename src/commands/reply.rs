use std::collections::VecDeque;
use std::str::FromStr;
use unicode_segmentation::UnicodeSegmentation;

use crate::commands::reply_code::ReplyCode;

#[derive(PartialEq, Clone, Debug)]
pub(crate) struct Reply {
  pub(crate) code: ReplyCode,
  lines: Vec<String>,
}

impl Reply {
  pub(crate) fn new(code: ReplyCode, message: impl Into<String>) -> Self {
    Reply {
      code,
      lines: vec![message.into()],
    }
  }

  pub(crate) fn new_multiline(code: ReplyCode, lines: Vec<impl Into<String>>) -> Self {
    let lines = lines.into_iter().map(|l| l.into()).collect();
    Reply { code, lines }
  }
}

impl ToString for Reply {
  fn to_string(&self) -> String {
    let mut buffer;
    if self.lines.len() < 2 {
      buffer = format!(
        "{} {}\r\n",
        self.code as u16,
        self.lines.first().unwrap_or(&String::new())
      );
      return buffer;
    }
    buffer = format!("{}-{}\r\n", self.code as u16, self.lines.first().unwrap());
    let end = self.lines.len() - 1;
    self.lines[1..end]
      .iter()
      .for_each(|l| buffer.push_str(&format!("{}\r\n", l)));
    buffer.push_str(&format!(
      "{} {}\r\n",
      self.code as u16,
      self.lines.last().unwrap_or(&String::new())
    ));
    buffer
  }
}

impl FromStr for Reply {
  type Err = anyhow::Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let mut lines = s.split_inclusive("\n").collect::<VecDeque<&str>>();
    if lines.len() == 0 || s.len() < 5 {
      anyhow::bail!("Reply too short!");
    }
    let start = lines.pop_front().unwrap();
    let delimiter = start.graphemes(true).nth(3).unwrap_or("\0");
    if delimiter != " " && delimiter != "-" {
      anyhow::bail!("Reply code must be followed by a whitespace or minus!");
    }
    let code = match start[0..3].parse::<u16>() {
      Ok(c) => {
        let reply_code = ReplyCode::from_repr(c);
        if reply_code.is_none() {
          anyhow::bail!("Unsupported reply code!");
        }
        reply_code.unwrap()
      }
      Err(e) => anyhow::bail!("Invalid reply code! {}", e),
    };
    if lines.is_empty() {
      return Ok(Reply::new(code, &start[4..].trim_end().to_string()));
    }
    let last = match lines.pop_back() {
      Some(l) => l,
      None => anyhow::bail!("Multiline message without end line!"),
    };

    if last.len() < 5 {
      anyhow::bail!("Invalid end line in multiline message!");
    }

    match last[0..3].parse::<u16>() {
      Ok(c) => {
        if c != code as u16 {
          anyhow::bail!("Codes in multiline message do not match!")
        }
      }
      Err(e) => anyhow::bail!("Invalid end line reply code! {}", e),
    };

    let mut parts: Vec<&str> = Vec::with_capacity(lines.len());
    parts.push(&start[4..].trim_end());
    lines.iter().for_each(|l| parts.push(l.trim_end()));
    parts.push(&last[4..].trim_end());

    Ok(Reply::new_multiline(code, parts))
  }
}

#[cfg(test)]
mod tests {
  use std::str::FromStr;

  use crate::commands::reply::Reply;
  use crate::commands::reply_code::ReplyCode;

  #[test]
  fn test_from_string_single_line() {
    let reply = Reply::new(ReplyCode::CommandOkay, "test");
    let message = reply.to_string();
    let parsed_reply = Reply::from_str(&message);
    match parsed_reply {
      Ok(r) => assert_eq!(reply, r),
      Err(e) => {
        panic!("Failed to parse! {}", e);
      }
    }
  }

  #[test]
  fn test_from_string_multiline() {
    let lines = vec!["Hello", "mid", "Bye"];
    let reply = Reply::new_multiline(ReplyCode::CommandOkay, lines);
    let message = reply.to_string();
    let parsed_reply = Reply::from_str(&message);
    match parsed_reply {
      Ok(r) => assert_eq!(reply, r),
      Err(e) => {
        panic!("Failed to parse! {}", e);
      }
    }
  }

  #[test]
  fn from_string_invalid_code_test() {
    let message = "0 Hello";
    let parsed_reply = Reply::from_str(message);
    assert!(parsed_reply.is_err());
  }

  #[test]
  fn from_string_invalid_no_space_or_minus_test() {
    let message = "220Hello";
    let parsed_reply = Reply::from_str(message);
    assert!(parsed_reply.is_err());
  }

  #[test]
  fn from_string_invalid_no_message_test() {
    let message = "220";
    let parsed_reply = Reply::from_str(message);
    assert!(parsed_reply.is_err());
  }

  #[test]
  fn from_string_invalid_no_code_test() {
    let message = "abc Hello";
    let parsed_reply = Reply::from_str(message);
    assert!(parsed_reply.is_err());
  }
}
