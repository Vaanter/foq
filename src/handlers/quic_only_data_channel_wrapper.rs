use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use s2n_quic::Connection;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::handlers::connection_handler::AsyncReadWrite;
use crate::handlers::data_channel_wrapper::DataChannelWrapper;

pub(crate) struct QuicOnlyDataChannelWrapper {
    addr: SocketAddr,
    data_channel: Arc<Mutex<Option<Box<dyn AsyncReadWrite>>>>,
    connection: Arc<Mutex<Connection>>,
}

impl QuicOnlyDataChannelWrapper {
    pub(crate) fn new(mut addr: SocketAddr, connection: Arc<Mutex<Connection>>) -> Self {
        addr.set_port(0);
        QuicOnlyDataChannelWrapper {
            addr,
            data_channel: Arc::new(Mutex::new(None)),
            connection,
        }
    }

    async fn create_stream(&mut self) -> Result<SocketAddr, Box<dyn Error>> {
        println!("Creating passive listener");
        let conn = self.connection.clone();
        let mut data_channel = self.data_channel.clone().lock_owned().await;
        tokio::spawn(async move {
            let conn = tokio::time::timeout(Duration::from_secs(20), {
                conn.lock().await.accept_bidirectional_stream()
            })
            .await;

            match conn {
                Ok(Ok(Some(stream))) => {
                    let _ = data_channel.insert(Box::new(stream));
                    println!("Passive listener connection successful!");
                }
                Ok(Ok(None)) => eprintln!("Connection closed while awaiting stream!"),
                Ok(Err(e)) => eprintln!("Passive listener connection failed! {e}"),
                Err(e) => {
                    eprintln!("Client failed to connect to passive listener before timeout! {e}")
                }
            };
        });

        Ok(self.addr.clone())
    }
}

#[async_trait]
impl DataChannelWrapper for QuicOnlyDataChannelWrapper {
    async fn open_data_stream(&mut self) -> Result<SocketAddr, Box<dyn Error>> {
        self.create_stream().await
    }

    async fn get_data_stream(&self) -> Arc<Mutex<Option<Box<dyn AsyncReadWrite>>>> {
        self.data_channel.clone()
    }

    async fn close_data_stream(&mut self) {
        let dc = self.data_channel.clone();
        if dc.lock().await.is_some() {
            let _ = dc.lock().await.as_mut().unwrap().shutdown().await;
        };
    }

    async fn get_addr(&self) -> &SocketAddr {
        &self.addr
    }
}
