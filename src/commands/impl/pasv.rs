use std::net::{SocketAddr, SocketAddrV4};
use std::sync::Arc;

use tracing::error;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::handlers::reply_sender::ReplySend;
use crate::session::command_processor::CommandProcessor;

#[tracing::instrument(skip(command_processor, reply_sender))]
pub(crate) async fn pasv(
  command: &Command,
  command_processor: Arc<CommandProcessor>,
  reply_sender: Arc<impl ReplySend>,
) {
  debug_assert_eq!(command.command, Commands::Pasv);

  let wrapper = command_processor.data_wrapper.clone();
  let properties = command_processor.session_properties.read().await;
  let reply = match wrapper.open_data_stream(properties.prot_mode).await {
    Ok(addr) => match addr {
      SocketAddr::V4(addr) => {
        Reply::new(ReplyCode::EnteringPassiveMode, create_pasv_response(&addr))
      }
      SocketAddr::V6(_) => {
        error!("PASV: IPv6 is not supported!");
        Reply::new(ReplyCode::CommandNotImplementedForThatParameter, "Server only supports IPv6!")
      }
    },
    Err(e) => {
      error!("Failed to open data stream: {}", e);
      Reply::new(ReplyCode::SyntaxErrorInParametersOrArguments, "Failed to listen for data stream")
    }
  };
  reply_sender.send_control_message(reply).await;
}

fn create_pasv_response(ip: &SocketAddrV4) -> String {
  let octets = ip.ip().octets();
  let p1 = ip.port() / 256;
  let p2 = ip.port() - p1 * 256;
  format!(
    "Entering Passive Mode ({},{},{},{},{},{})",
    octets[0], octets[1], octets[2], octets[3], p1, p2
  )
}

#[cfg(test)]
mod tests {
  use std::env::current_dir;
  use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
  use std::str::FromStr;
  use std::sync::Arc;
  use std::time::Duration;

  use tokio::net::TcpStream;
  use tokio::sync::mpsc::channel;
  use tokio::time::timeout;

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::r#impl::pasv::create_pasv_response;
  use crate::commands::reply::Reply;
  use crate::commands::reply_code::ReplyCode;
  use crate::utils::test_utils::{
    CommandProcessorSettingsBuilder, TestReplySender, setup_test_command_processor_custom,
  };

  #[test]
  fn response_test() {
    let ip = SocketAddrV4::new(Ipv4Addr::from([127, 0, 0, 1]), 55692);
    assert_eq!(create_pasv_response(&ip), "Entering Passive Mode (127,0,0,1,217,140)");
  }

  #[tokio::test]
  async fn simple_open_dc() {
    let command = Command::new(Commands::Pasv, String::new());

    let label = "test_files".to_string();

    let settings = CommandProcessorSettingsBuilder::default()
      .label(label.clone())
      .change_path(Some(label.clone()))
      .username(Some("testuser".to_string()))
      .view_root(current_dir().unwrap())
      .build()
      .expect("Settings should be valid");

    let command_processor = setup_test_command_processor_custom(&settings);

    let (tx, mut rx) = channel(1024);
    let reply_sender = TestReplySender::new(tx);
    if timeout(
      Duration::from_secs(2),
      command.execute(Arc::new(command_processor), Arc::new(reply_sender)),
    )
    .await
    .is_err()
    {
      panic!("Command timeout!");
    };

    let reply = match timeout(Duration::from_secs(5), rx.recv()).await {
      Ok(Some(reply)) => {
        assert_eq!(reply.code, ReplyCode::EnteringPassiveMode);
        assert_eq!(reply.to_string().trim_end().chars().filter(|c| *c == ',').count(), 5);
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
    let start = message.find('(').expect("Address should start with '(' (non-standard)");
    let end = message.find(')').expect("Address should end with ')' (non-standard)");
    let addr = message[start + 1..end].split(',').collect::<Vec<&str>>();

    let mut ip = addr.iter().copied().take(4).fold(String::with_capacity(16), |mut a, b| {
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
