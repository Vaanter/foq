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
use crate::io::session::Session;

pub(crate) struct Pasv;

#[async_trait]
impl Executable for Pasv {
  async fn execute(session: &mut Session, command: &Command, reply_sender: &mut impl ReplySend) {
    debug_assert_eq!(command.command, Commands::PASV);

    match timeout(Duration::from_secs(5), session.data_wrapper.clone().lock()).await {
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
      Err(e) => {
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
  use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
  use std::sync::Arc;
  use std::time::Duration;

  use tokio::net::TcpStream;
  use tokio::sync::mpsc::channel;
  use tokio::sync::Mutex;
  use tokio::time::timeout;
  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;

  use crate::commands::r#impl::pasv::Pasv;
  use crate::handlers::data_wrapper::DataChannelWrapper;
  use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::reply_code::ReplyCode;
  use crate::io::session::Session;
  use crate::utils::test_utils::TestReplySender;

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
    let ip: SocketAddr = "127.0.0.1:53245"
      .parse()
      .expect("Test listener requires available IP:PORT");

    let command = Command::new(Commands::PASV, String::new());

    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let mut session = Session::new_with_defaults(wrapper.clone());

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(e) = timeout(
      Duration::from_secs(2),
      Pasv::execute(&mut session, &command, &mut reply_sender),
    )
      .await
    {
      panic!("Command timeout!");
    };

    println!("Connecting to passive listener");
    let mut client_dc = match TcpStream::connect(ip).await {
      Ok(c) => c,
      Err(e) => {
        panic!("Client passive connection failed: {}", e);
      }
    };
    println!("Client passive connection successful!");

    match timeout(Duration::from_secs(5), rx.recv()).await {
      Ok(Some(reply)) => {
        assert_eq!(reply.code, ReplyCode::DataConnectionOpen);
        assert!(reply.to_string().trim_end().chars().filter(|c| *c == ',').count() > 5);
      }
      Ok(None) => {
        panic!("No reply received!");
      }
      Err(e) => {
        panic!("Reply timeout! {}", e);
      }
    }
  }
}
