use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use s2n_quic::stream::BidirectionalStream;
use s2n_quic::Connection;
use tokio::io::{AsyncBufReadExt, BufReader, ReadHalf};
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::timeout;

use crate::handlers::connection_handler::ConnectionHandler;
use crate::handlers::quic_only_data_channel_wrapper::QuicOnlyDataChannelWrapper;
use crate::handlers::reply_sender::ReplySender;
use crate::io::reply::Reply;
use crate::io::session::Session;

pub(crate) struct QuicOnlyConnectionHandler {
  pub(crate) connection: Arc<Mutex<Connection>>,
  data_channel_wrapper: Arc<Mutex<QuicOnlyDataChannelWrapper>>,
  session: Arc<Mutex<Session>>,
  shutdown_receiver: Receiver<()>,
  control_channel: Option<BufReader<ReadHalf<BidirectionalStream>>>,
  reply_loop: Option<JoinHandle<()>>,
  reply_sender: Option<ReplySender<BidirectionalStream>>,
  tx: Option<Sender<Reply>>,
}

impl QuicOnlyConnectionHandler {
  pub(crate) fn new(connection: Connection, shutdown_receiver: Receiver<()>) -> Self {
    let addr = connection.local_addr().unwrap();
    let connection = Arc::new(Mutex::new(connection));
    let wrapper = Arc::new(Mutex::new(QuicOnlyDataChannelWrapper::new(
      addr,
      connection.clone(),
    )));
    let session = Arc::new(Mutex::new(Session::new_with_defaults(wrapper.clone())));

    QuicOnlyConnectionHandler {
      connection,
      data_channel_wrapper: wrapper,
      session,
      shutdown_receiver,
      control_channel: None,
      reply_loop: None,
      reply_sender: None,
      tx: None,
    }
  }

  pub(crate) fn get_session(&self) -> Arc<Mutex<Session>> {
    self.session.clone()
  }

  async fn await_command(&mut self) -> Option<Reply> {
    let mut buf = String::new();
    println!("Server reading command!");
    let cc = self
      .control_channel
      .as_mut()
      .expect("Control channel must be open to receive commands!");
    let bytes = match cc.read_line(&mut buf).await {
      Ok(len) => {
        println!("Server command read!");
        len
      }
      Err(e) => {
        eprintln!("Failed to read command! {}", e);
        0
      }
    };
    if bytes > 0usize {
      let session = self.get_session();
      session
        .lock()
        .await
        .evaluate(buf, self.reply_sender.as_mut().unwrap())
        .await;
      return None;
    }
    None
  }

  async fn create_control_channel(&mut self) -> Result<(), anyhow::Error> {
    let conn = self.connection.clone();
    let result = match timeout(
      Duration::from_secs(10),
      conn.lock().await.accept_bidirectional_stream(),
    )
    .await
    {
      Ok(Ok(Some(stream))) => Ok(stream),
      Ok(Ok(None)) => Err(anyhow::anyhow!(
        "Connection closed while awaiting control stream!"
      )),
      Ok(Err(e)) => Err(anyhow::anyhow!(e)),
      Err(e) => Err(anyhow::anyhow!(e)),
    };

    return match result {
      Ok(control_channel) => {
        let (reader, writer) = tokio::io::split(control_channel);
        let control_channel = BufReader::new(reader);
        let reply_sender = ReplySender::new(writer);
        let _ = self.control_channel.insert(control_channel);
        let _ = self.reply_sender.insert(reply_sender);
        Ok(())
      }
      Err(e) => Err(e),
    };
  }
}

impl Drop for QuicOnlyConnectionHandler {
  fn drop(&mut self) {
    println!("QuicOnlyConnectionHandler dropped");
  }
}

#[async_trait]
impl ConnectionHandler for QuicOnlyConnectionHandler {
  async fn handle(&mut self, mut receiver: Receiver<()>) -> Result<(), anyhow::Error> {
    println!("Quic handler execute!");

    self.create_control_channel().await?;

    loop {
      tokio::select! {
        reply = self.await_command() => {
          if reply.is_some() {

          }
        },
        _ = receiver.recv() => {
          println!("Shutdown received!");
          if let Ok(conn) = timeout(Duration::from_secs(2), self.connection.clone().lock_owned()).await {
            conn.close(0u32.into())
          };
          break;
        }
      }
    }

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use s2n_quic::client::Connect;
  use s2n_quic::Client;
  use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
  use tokio::time::timeout;

  use crate::handlers::data_wrapper::DataChannelWrapper;
  use crate::handlers::quic_only_connection_handler::QuicOnlyConnectionHandler;
  use crate::listeners::quic_only_listener::QuicOnlyListener;

  pub static CERT_PEM: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../foq/certs/server-cert.pem"
  ));

  #[tokio::test]
  async fn smoke() {
    let server_addr = "127.0.0.1:0"
      .parse()
      .expect("Test listener requires available IP:PORT");

    let (shutdown_send, shutdown_recv) = tokio::sync::broadcast::channel(1024);
    let mut listener = QuicOnlyListener::new(server_addr, shutdown_recv).unwrap();

    let addr = listener.server.local_addr().unwrap();
    println!("Server port is {}", addr.port());
    let handler_shutdown_recv = shutdown_send.subscribe();
    let (port_send, port_recv) = tokio::sync::oneshot::channel();
    let fut = tokio::spawn(async move {
      let conn = listener.accept().await.unwrap();
      let mut handler = QuicOnlyConnectionHandler::new(conn, handler_shutdown_recv);
      let wrapper = handler.data_channel_wrapper.clone();

      let control_channel = match handler.create_control_channel().await {
        Ok(cc) => cc,
        Err(_) => panic!("Failed to create control channel!"),
      };

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

    let client = Client::builder()
      .with_tls(CERT_PEM)
      .expect("Client requires valid TLS settings!")
      .with_io("0.0.0.0:0")
      .expect("Client requires valid I/O settings!")
      .start()
      .expect("Client must be able to start");

    let connect = Connect::new(addr).with_server_name("localhost");
    let mut connection = match client.connect(connect).await {
      Ok(conn) => conn,
      Err(e) => {
        panic!("Client failed to connect to the server! {}", e);
      }
    };

    let client_cc = match connection.open_bidirectional_stream().await {
      Ok(c) => c,
      Err(e) => {
        panic!("Client failed to open control channel bidi! {}", e);
      }
    };

    let port_msg = timeout(Duration::from_secs(5), port_recv).await;

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
    let client_dc = timeout(
      std::time::Duration::from_secs(5),
      connection.open_bidirectional_stream(),
    )
    .await;
    if let Err(e) = client_dc {
      panic!("Client passive connection failed: {}", e);
    }

    let (reader, writer) = client_cc.split();
    let message = "MLSD\r\n";
    let mut client_writer = BufWriter::new(writer);

    match client_writer.write(message.as_ref()).await {
      Ok(len) => {
        if let Err(e) = client_writer.flush().await {
          eprintln!("Flushing client_writer failed! {}", e);
        };
        println!("Client message sent! Bytes: {}", len);
      }
      Err(e) => {
        panic!("Client message failed to send: {}", e);
      }
    }

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
    client_dc
      .unwrap()
      .unwrap()
      .read_to_string(&mut data_buf)
      .await
      .unwrap();
    println!("{}", data_buf);

    fut.await.expect("fut fail");
  }
}
