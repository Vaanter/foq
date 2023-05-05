use std::net::{SocketAddr, SocketAddrV4};
use std::time::Duration;

use async_trait::async_trait;
use tokio::time::timeout;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;
use crate::io::command_processor::CommandProcessor;

pub(crate) struct Pasv;

#[async_trait]
impl Executable for Pasv {
  async fn execute(command_processor: &mut CommandProcessor, command: &Command, reply_sender: &mut impl ReplySend) {
    debug_assert_eq!(command.command, Commands::PASV);

    match timeout(Duration::from_secs(5), command_processor.data_wrapper.clone().lock()).await {
      Ok(mut wrapper) => {
        let reply = match wrapper.open_data_stream().await.unwrap() {
          SocketAddr::V4(addr) => Reply::new(
            ReplyCode::EnteringPassiveMode,
            &Pasv::create_pasv_response(&addr),
          ),
          SocketAddr::V6(_) => {
            eprintln!("PASV: IPv6 is not supported!");
            Reply::new(
              ReplyCode::CommandNotImplementedForThatParameter,
              "Server only supports IPv6!",
            )
          }
        };
        Pasv::reply(reply, reply_sender).await;
      }
      Err(_) => {
        panic!("Wrapper is not available! TF?!");
      }
    }
  }
}

impl Pasv {
  pub(crate) fn create_pasv_response(ip: &SocketAddrV4) -> String {
    let octets = ip.ip().octets();
    let p1 = ip.port() / 256;
    let p2 = ip.port() - p1 * 256;
    format!(
      "Entering Passive Mode ({},{},{},{},{},{})",
      octets[0], octets[1], octets[2], octets[3], p1, p2
    )
  }
}

#[cfg(test)]
mod tests {
  use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
  use std::str::FromStr;
  use std::sync::Arc;
  use std::time::Duration;

  use tokio::net::TcpStream;
  use tokio::sync::mpsc::channel;
  use tokio::sync::{Mutex, RwLock};
  use tokio::time::timeout;

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::pasv::Pasv;
  use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::reply::Reply;
  use crate::io::reply_code::ReplyCode;
  use crate::io::command_processor::CommandProcessor;
  use crate::io::session_properties::SessionProperties;
  use crate::utils::test_utils::{TestReplySender, LOCALHOST};

  #[test]
  fn response_test() {
    let ip = SocketAddrV4::new(Ipv4Addr::from([127, 0, 0, 1]), 55692);
    assert_eq!(
      Pasv::create_pasv_response(&ip),
      "Entering Passive Mode (127,0,0,1,217,140)"
    );
  }

  #[tokio::test]
  async fn simple_open_dc() {
    let command = Command::new(Commands::PASV, String::new());

    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(LOCALHOST)));
    let session_properties = Arc::new(RwLock::new(SessionProperties::new()));
    let mut session = CommandProcessor::new(session_properties, wrapper.clone());

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(_) = timeout(
      Duration::from_secs(2),
      Pasv::execute(&mut session, &command, &mut reply_sender),
    )
    .await
    {
      panic!("Command timeout!");
    };

    let reply = match timeout(Duration::from_secs(5), rx.recv()).await {
      Ok(Some(reply)) => {
        assert_eq!(reply.code, ReplyCode::EnteringPassiveMode);
        assert_eq!(
          reply
            .to_string()
            .trim_end()
            .chars()
            .filter(|c| *c == ',')
            .count(),
          5
        );
        reply
      }
      Ok(None) => {
        panic!("No reply received!");
      }
      Err(e) => {
        panic!("Reply timeout! {}", e);
      }
    };

    let addr = parse_socketaddr(reply);

    println!("Connecting to passive listener");
    if let Err(e) = TcpStream::connect(addr).await {
      panic!("Client passive connection failed: {}", e);
    };
    println!("Client passive connection successful!");
  }

  fn parse_socketaddr(reply: Reply) -> SocketAddr {
    let message = reply.to_string();
    let start = message
      .find("(")
      .expect("Address should start with '(' (non-standard)");
    let end = message
      .find(")")
      .expect("Address should end with ')' (non-standard)");
    let addr = message[start + 1..end].split(",").collect::<Vec<&str>>();

    let mut ip = addr
      .iter()
      .copied()
      .take(4)
      .fold(String::with_capacity(16), |mut a, b| {
        a.push_str(b);
        a.push('.');
        a
      });
    ip.pop(); // remove trailing dot

    let p1 = addr
      .get(4)
      .copied()
      .expect("Address should contain p1")
      .parse::<u16>()
      .expect("p1 should be valid integer");
    let p2 = addr
      .get(5)
      .copied()
      .expect("Address should contain p2")
      .parse::<u16>()
      .expect("p2 should be valid integer");

    let addr =
      IpAddr::V4(Ipv4Addr::from_str(&ip).expect("Message should contain valid IPv4 octets"));
    SocketAddr::new(addr, p1 * 256 + p2)
  }
}
