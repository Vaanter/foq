use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader, ReadHalf};
use tokio::net::TcpStream;
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::{channel, Sender};
use tokio::sync::Mutex;
use tokio::time::sleep;

use crate::commands::executable::Executable;
use crate::commands::r#impl::noop::Noop;
use crate::handlers::connection_handler::ConnectionHandler;
use crate::handlers::reply_sender::ReplySender;
use crate::handlers::standard_data_channel_wrapper::StandardDataChannelWrapper;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;
use crate::io::session::Session;

pub(crate) struct StandardConnectionHandler {
  data_channel_wrapper: Arc<Mutex<StandardDataChannelWrapper>>,
  session: Arc<Mutex<Session>>,
  control_channel: BufReader<ReadHalf<TcpStream>>,
  reply_sender: ReplySender<TcpStream>,
  tx: Sender<Reply>,
}

impl StandardConnectionHandler {
  pub(crate) fn new(stream: TcpStream) -> Self {
    let wrapper = Arc::new(Mutex::new(StandardDataChannelWrapper::new(
      stream.local_addr().unwrap().clone(),
    )));
    let stream_halves = tokio::io::split(stream);
    let control_channel = BufReader::new(stream_halves.0);
    let (tx, rx) = channel(8096);
    let reply_sender = ReplySender::new(stream_halves.1);
    let session = Arc::new(Mutex::new(Session::new_with_defaults(wrapper.clone())));
    StandardConnectionHandler {
      data_channel_wrapper: wrapper,
      session,
      control_channel,
      reply_sender,
      tx,
    }
  }

  pub(crate) fn get_session(&self) -> Arc<Mutex<Session>> {
    self.session.clone()
  }

  pub(crate) async fn await_command(&mut self) -> Result<(), anyhow::Error> {
    let reader = &mut self.control_channel;
    let mut buf = String::new();
    println!("Server reading command!");
    let bytes = match reader.read_line(&mut buf).await {
      Ok(len) => {
        println!("Server command read! {}", len);
        len
      }
      Err(e) => {
        eprintln!("Failed to read command! {}", e);
        0
      }
    };
    if bytes == 0 {
      anyhow::bail!("Connection closed!");
    }

    let session = self.session.clone();
    //tokio::spawn(async move {
    session
      .lock()
      .await
      .evaluate(buf, &mut self.reply_sender)
      .await;
    sleep(Duration::from_secs(1)).await;
    //});
    Ok(())
  }
}

#[async_trait]
impl ConnectionHandler for StandardConnectionHandler {
  async fn handle(&mut self, mut receiver: Receiver<()>) -> Result<(), anyhow::Error> {
    println!("Standard handler execute!");

    let hello = Reply::new(ReplyCode::ServiceReady, "Hello");
    Noop::reply(hello, &mut self.reply_sender).await;

    loop {
      tokio::select! {
        result = self.await_command() => {
          if let Err(e) = result {
            println!("{}", e);
            break;
          }
        },
        _ = receiver.recv() => {
          println!("Shutdown received!");
          //let _ = timeout(Duration::from_secs(2), self.control_channel.0.shutdown());
          //let _ = timeout(Duration::from_secs(2), self.control_channel.1.shutdown());
          break;
        }
      }
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use std::net::SocketAddr;
  use std::time::Duration;

  use tokio::io;
  use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
  use tokio::net::{TcpListener, TcpStream};
  use tokio::time::timeout;

  use crate::handlers::data_channel_wrapper::DataChannelWrapper;
  use crate::handlers::standard_connection_handler::StandardConnectionHandler;

  #[tokio::test]
  async fn smoke() {
    let ip: SocketAddr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");

    let listener = TcpListener::bind(ip).await;
    assert!(listener.is_ok());
    let listener = listener.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
      loop {
        let _ = listener.accept().await;
      }
    });

    let stream = TcpStream::connect(addr).await.unwrap();
    let handler = StandardConnectionHandler::new(stream);

    let (port_send, port_recv) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
      let result = handler
        .data_channel_wrapper
        .clone()
        .lock()
        .await
        .open_data_stream()
        .await;
      assert!(result.is_ok());
      port_send.send(result.unwrap())
    });

    let port_msg = timeout(Duration::from_secs(3), port_recv).await;

    let addr = match port_msg {
      Ok(Ok(addr)) => {
        println!("Address is {}", addr);
        addr
      }
      Ok(Err(e)) => {
        panic!("Failed to receive port: {}", e);
      }
      Err(e) => {
        panic!("Failed to receive port: {}", e);
      }
    };
    println!("Connecting to passive listener");
    let pass: io::Result<TcpStream> = TcpStream::connect(addr).await;
    if let Err(e) = pass.as_ref() {
      assert!(false, "{}", e);
    }
    println!("Connection successful!");
  }

  #[tokio::test]
  async fn test_command() {
    let ip: SocketAddr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");

    let listener = match TcpListener::bind(ip).await {
      Ok(l) => l,
      Err(e) => {
        panic!("Failed to create server listener! {}", e);
      }
    };
    let addr = listener.local_addr().unwrap();
    println!("Server port is {}", addr.port());
    let (port_send, port_recv) = tokio::sync::oneshot::channel();
    let fut = tokio::spawn(async move {
      let (server_cc, _) = listener.accept().await.unwrap();
      let mut handler = StandardConnectionHandler::new(server_cc);

      let wrapper = handler.data_channel_wrapper.clone();
      tokio::spawn(async move {
        let result = wrapper.lock().await.open_data_stream().await;
        assert!(result.is_ok());
        port_send.send(result.unwrap())
      });

      match timeout(Duration::from_secs(5), handler.await_command()).await {
        Ok(_) => {
          println!("Command received!");
        }
        Err(e) => {
          panic!("Failed to receive command!, {}", e);
        }
      };
    });

    let mut client_cc = TcpStream::connect(addr).await.unwrap();
    let port_msg = timeout(Duration::from_secs(3), port_recv).await;

    let addr = match port_msg {
      Ok(Ok(addr)) => {
        println!("Address is {}", addr);
        addr
      }
      Ok(Err(e)) => {
        panic!("Failed to receive port: {}", e);
      }
      Err(e) => {
        panic!("Failed to receive port: {}", e);
      }
    };

    println!("Connecting to passive listener");
    let mut client_dc = match TcpStream::connect(addr).await {
      Ok(c) => c,
      Err(e) => {
        panic!("Client passive connection failed: {}", e);
      }
    };
    println!("Client passive connection successful!");

    let (reader, writer) = client_cc.split();
    let message = "MLSD\r\n";
    let mut client_writer = BufWriter::new(writer);
    match client_writer.write(message.as_ref()).await {
      Ok(len) => {
        client_writer
          .flush()
          .await
          .expect("Flushing client message should work!");
        println!("Client message sent! Bytes: {}", len);
      }
      Err(e) => {
        panic!("Client message failed to send: {}", e);
      }
    };

    let mut client_reader = BufReader::new(reader);
    let mut buffer = String::new();
    match client_reader.read_line(&mut buffer).await {
      Ok(len) => {
        println!("Received reply from server!: {}", buffer.trim());
        assert!(buffer.trim().starts_with("150"));
        buffer.clear();
      }
      Err(e) => {
        panic!("Failed to read reply! {}", e);
      }
    }

    match client_reader.read_line(&mut buffer).await {
      Ok(_len) => {
        println!("Received reply from server!: {}", buffer.trim());
        assert!(buffer.trim().starts_with("226"));
      }
      Err(e) => {
        panic!("Failed to read reply! {}", e);
      }
    }

    let mut data_buf = String::new();
    client_dc.read_to_string(&mut data_buf).await.unwrap();
    println!("{}", data_buf);

    fut.await.expect("fut fail");
  }
}
