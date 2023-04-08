use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use s2n_quic::stream::BidirectionalStream;
use s2n_quic::Server;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

// src/bin/server.rs
use crate::handlers::connection_handler::ConnectionHandler;
use crate::handlers::quic_only_connection_handler::QuicOnlyConnectionHandler;
use crate::handlers::standard_connection_handler::StandardConnectionHandler;
use crate::listeners::quic_only_listener::QuicOnlyListener;
use crate::listeners::standard_listener::StandardListener;

/// NOTE: this certificate is to be used for demonstration purposes only!
pub static CERT_PEM: &str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/certs/server-cert.pem"
));
/// NOTE: this certificate is to be used for demonstration purposes only!
pub static KEY_PEM: &str =
  include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/certs/server-key.pem"));

pub(crate) async fn get_port() {
  let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
  let listener = TcpListener::bind(addr).await;
  println!("Port: {}", listener.unwrap().local_addr().unwrap().port());
}

pub(crate) async fn run() {
  run_tcp_listener().await;
}

#[allow(unused)]
async fn run_tcp_listener() {
  let (shutdown_send, shutdown_recv) = tokio::sync::broadcast::channel(1024);
  let addr = "127.0.0.1:8765".parse().unwrap();
  let mut standard_listener = StandardListener::new(addr, shutdown_send.subscribe()).unwrap();

  match standard_listener.accept().await {
    Ok(stream) => {
      println!("Received connection");
      let mut handler = StandardConnectionHandler::new(stream);
      handler.handle(shutdown_send.subscribe()).await;
    }
    Err(e) => {
      eprintln!("Error {e}")
    }
  }
}

#[allow(unused)]
async fn run_quic_listener() {
  let (shutdown_send, shutdown_recv) = tokio::sync::broadcast::channel(1024);
  let addr = "127.0.0.1:9876".parse().unwrap();
  let mut quic_only_listener = QuicOnlyListener::new(addr, shutdown_send.subscribe()).unwrap();

  let handler_shutdown_recv = shutdown_send.subscribe();
  tokio::spawn(async move {
    match quic_only_listener.accept().await {
      Ok(conn) => {
        println!("Received connection!");
        let handler = QuicOnlyConnectionHandler::new(conn);
      }
      Err(e) => {
        eprintln!("Error {e}")
      }
    }
  });

  match tokio::signal::ctrl_c().await {
    Ok(_) => {
      shutdown_send.send(()).unwrap();
    }
    Err(_) => {}
  }
}

#[allow(unused)]
async fn run_tcp() -> Result<(), std::io::Error> {
  let listener = TcpListener::bind("127.0.0.1:8080").await?;
  println!("Listening on: {}", listener.local_addr().unwrap());

  loop {
    let (socket, _) = listener.accept().await?;

    tokio::spawn(async move {
      handle_tcp_connection(socket).await.unwrap();
    });
  }
}

async fn handle_tcp_connection(mut stream: TcpStream) -> Result<(), std::io::Error> {
  let mut buffer = [0; 1024];
  let len = stream.read(&mut buffer).await?;

  let message = String::from_utf8_lossy(&buffer[..len]);
  println!("Received: {}", message);

  let _ = stream.write_all(message.as_bytes()).await;
  println!("Sent: {}", message);

  Ok(())
}

#[allow(unused)]
async fn run_quic() -> Result<(), Box<dyn Error>> {
  let mut server = Server::builder()
    .with_tls((CERT_PEM, KEY_PEM))?
    .with_io("127.0.0.1:4433")?
    .start()?;

  loop {
    match server.accept().await {
      Some(mut connection) => {
        println!("Creating task for connection.");
        // spawn a new task for the connection
        tokio::spawn(async move {
          eprintln!("Connection accepted from {:?}", connection.remote_addr());

          loop {
            eprintln!("Accepting stream!");
            match connection.accept_bidirectional_stream().await {
              Ok(Some(stream)) => {
                handle_stream(stream);
              }
              Ok(None) => {
                eprintln!("Connection closed when waiting for stream");
                break;
              }
              Err(e) => {
                eprintln!("Error opening stream! {}", e);
                break;
              }
            }
          }

          eprintln!("Connection closed!");
        });
      }
      None => {
        eprintln!("server closed!");
        break;
      }
    }
  }

  Ok(())
}

#[allow(unused)]
fn handle_stream(mut stream: BidirectionalStream) {
  tokio::spawn(async move {
    eprintln!("Stream opened from {:?}", stream.connection().remote_addr());

    loop {
      match stream.receive().await {
        Ok(Some(data)) => {
          println!("Data received!");
          // echo any data back to the stream
          eprintln!("Received: {}", std::str::from_utf8(&data).unwrap());
          let current = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string();
          println!("Sent: {}", current);
          let msg = Bytes::from(current);
          stream.send(msg).await.expect("stream should be open");
        }
        Ok(None) => {
          eprintln!("Stream closed!");
          break;
        }
        Err(e) => {
          eprintln!("Stream error: {}", e);
          break;
        }
      }
    }
    //eprintln!("Closing stream!");
    //stream.stop_sending(0u8.into()).unwrap();
    //stream.close().await.unwrap();
  });
}

fn impl_test(x: Arc<Mutex<impl ToString>>) {}
