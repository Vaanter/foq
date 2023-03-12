use std::path::Path;

use async_trait::async_trait;
use strum_macros::Display;
use tokio::io::AsyncWriteExt;

use crate::commands::command::Command;
use crate::commands::commands::Commands;
use crate::commands::executable::Executable;
use crate::handlers::reply_sender::ReplySend;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;
use crate::io::session::Session;

pub(crate) struct Mlsd;

#[derive(Ord, PartialOrd, Eq, PartialEq, Display)]
#[strum(serialize_all = "lowercase")]
pub(crate) enum EntryType {
  FILE,
  DIR,
  CDIR,
  PDIR,
  LINK,
}

pub(crate) struct EntryData {
  size: u64,
  entry_type: EntryType,
  perm: String,
  modify: u128,
  name: String,
}

impl ToString for EntryData {
  fn to_string(&self) -> String {
    let mut buffer = String::new();
    buffer.push_str(&format!("size={};", self.size));
    buffer.push_str(&format!("type={};", self.entry_type));
    buffer.push_str(&format!("modify={};", self.modify));
    buffer.push_str(&format!("perm={};", self.perm));
    buffer.push_str(&format!(" {}", self.name));
    buffer.push('\r');
    buffer.push('\n');
    buffer
  }
}

impl Mlsd {
  fn get_formatted_dir_listing(path: &Path) -> Vec<EntryData> {
    let directory_contents = path.read_dir();
    match directory_contents {
      Ok(entries) => {
        let mut listing: Vec<EntryData> = vec![];
        // TODO make this normal
        let entries = path
          .parent()
          .unwrap()
          .read_dir()
          .unwrap()
          .filter(|e| e.as_ref().unwrap().path() == path)
          .chain(entries);
        for entry in entries {
          let metadata = entry.as_ref().unwrap().metadata().unwrap();
          let (entry_type, perm) = {
            // TODO better permission lookup
            if entry.as_ref().unwrap().path() == path {
              (EntryType::CDIR, "cdefp")
            } else if entry.as_ref().unwrap().path() == path.parent().unwrap() {
              (EntryType::PDIR, "cdefp")
            } else if metadata.is_dir() {
              (EntryType::DIR, "cdefp")
            } else if metadata.is_file() {
              (EntryType::FILE, "adefrw")
            } else if metadata.is_symlink() {
              (EntryType::LINK, "fr")
            } else {
              panic!("Unknown file type!");
            }
          };
          listing.push(EntryData {
            size: metadata.len(),
            entry_type,
            perm: perm.to_string(),
            modify: metadata.modified().unwrap().elapsed().unwrap().as_nanos(),
            name: entry.as_ref().unwrap().file_name().into_string().unwrap(),
          });
        }
        return listing;
      }
      Err(e) => {
        eprintln!("Directory listing failed! That's a big problem! {e}");
        panic!("Listing failed TF?");
      }
    }
  }
}

#[async_trait]
impl Executable for Mlsd {
  async fn execute(session: &mut Session, command: &Command, reply_sender: &mut impl ReplySend) {
    debug_assert_eq!(command.command, Commands::MLSD);

    Mlsd::reply(
      Reply::new(
        ReplyCode::FileStatusOkay,
        "Transferring directory information!",
      ),
      reply_sender,
    )
    .await;
    let cwd = &session.cwd;
    println!("Getting listing!");
    let listing = Mlsd::get_formatted_dir_listing(cwd);
    println!("Getting data stream");
    let stream = session.data_wrapper.lock().await.get_data_stream().await;

    match stream.lock().await.as_mut() {
      Some(s) => {
        let mem = listing.iter().map(|l| l.to_string()).collect::<String>();
        println!("Writing to data stream");
        let _ = s.write_all(mem.as_ref()).await;
      }
      None => {
        eprintln!("Data stream non existent!");
      }
    }

    println!("Written to data stream");
    session.data_wrapper.lock().await.close_data_stream().await;
    Mlsd::reply(
      Reply::new(
        ReplyCode::ClosingDataConnection,
        "Directory information sent!",
      ),
      reply_sender,
    )
    .await;
  }
}

#[cfg(test)]
mod tests {
  use std::net::SocketAddr;
  use std::sync::Arc;
  use std::time::Duration;

  use tokio::io::AsyncReadExt;
  use tokio::net::TcpStream;
  use tokio::sync::mpsc::channel;
  use tokio::sync::Mutex;
  use tokio::time::timeout;

  use crate::commands::command::Command;
  use crate::commands::commands::Commands;
  use crate::commands::executable::Executable;
  use crate::commands::r#impl::mlsd::Mlsd;
  use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
  use crate::io::reply_code::ReplyCode;
  use crate::io::session::Session;
  use crate::utils::test_utils::TestReplySender;

  #[test]
  fn smoke() {
    let cwd = std::env::current_dir().unwrap();
    let listing = Mlsd::get_formatted_dir_listing(&cwd);
    println!(
      "{}",
      listing.iter().map(|l| l.to_string()).collect::<String>()
    );
  }

  #[tokio::test]
  async fn simple_listing_tcp() {
    let ip: SocketAddr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");

    let command = Command::new(Commands::MLSD, String::new());

    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(ip)));
    let mut session = Session::new_with_defaults(wrapper);
    let addr = match session
      .data_wrapper
      .clone()
      .lock()
      .await
      .open_data_stream()
      .await
    {
      Ok(addr) => addr,
      Err(_) => panic!("Failed to open passive data listener!"),
    };

    println!("Connecting to passive listener");
    let mut client_dc = match TcpStream::connect(addr).await {
      Ok(c) => c,
      Err(e) => {
        panic!("Client passive connection failed: {}", e);
      }
    };
    println!("Client passive connection successful!");

    let (tx, mut rx) = channel(1024);
    let mut reply_sender = TestReplySender::new(tx);
    if let Err(e) = timeout(
      Duration::from_secs(2),
      Mlsd::execute(&mut session, &command, &mut reply_sender),
    )
    .await
    {
      panic!("Command timeout!");
    };
    let mut buffer = [0; 1024];
    match timeout(Duration::from_secs(5), client_dc.read(&mut buffer)).await {
      Ok(Ok(len)) => {
        let msg = String::from_utf8_lossy(&buffer[..len]);
        assert!(!msg.is_empty());

        let file_count = std::env::current_dir()
          .expect("Current path should be available")
          .read_dir()
          .expect("Failed to read current path!")
          .count()
          + 1; // Add 1 to account for current path (.)
        assert_eq!(file_count, msg.lines().count());
      }
      Ok(Err(e)) => {
        assert!(false, "{}", e);
      }
      Err(e) => {
        assert!(false, "{}", e);
      }
    };
    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::FileStatusOkay);
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply in time!");
      }
    };

    match timeout(Duration::from_secs(2), rx.recv()).await {
      Ok(Some(result)) => {
        assert_eq!(result.code, ReplyCode::ClosingDataConnection);
      }
      Err(_) | Ok(None) => {
        panic!("Failed to receive reply in time!");
      }
    };
  }
}
