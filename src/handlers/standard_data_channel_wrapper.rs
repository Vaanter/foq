use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::time::timeout;

use crate::handlers::connection_handler::AsyncReadWrite;
use crate::handlers::data_wrapper::DataChannelWrapper;

pub(crate) struct StandardDataChannelWrapper {
    addr: SocketAddr,
    data_channel: Arc<Mutex<Option<Box<dyn AsyncReadWrite>>>>,
}

impl StandardDataChannelWrapper {
    pub(crate) fn new(mut addr: SocketAddr) -> Self {
        addr.set_port(0);
        StandardDataChannelWrapper {
            addr,
            data_channel: Arc::new(Mutex::new(None)),
        }
    }

    async fn create_stream(&mut self) -> Result<SocketAddr, Box<dyn Error>> {
        println!("Creating passive listener");
        let listener = TcpListener::bind(self.addr)
            .await
            .expect("Implement passive port search!");
        let port = listener.local_addr()?.port();
        let mut data_lock = self.data_channel.clone().lock_owned().await;
        tokio::spawn(async move {
            let conn = timeout(Duration::from_secs(20), {
                println!("Awaiting passive connection");
                listener.accept()
            })
            .await;
            match conn {
                Ok(Ok((stream, _))) => {
                    let _ = data_lock.insert(Box::new(stream));
                    println!("Passive listener connection successful!");
                }
                Ok(Err(e)) => {
                    eprintln!("Passive listener connection failed! {e}");
                }
                Err(e) => {
                    eprintln!("Client failed to connect to passive listener before timeout! {e}");
                }
            };
        });

        let mut addr = self.addr.clone();
        addr.set_port(port);
        Ok(addr)
    }
}

#[async_trait]
impl DataChannelWrapper for StandardDataChannelWrapper {
    async fn open_data_stream(&mut self) -> Result<SocketAddr, Box<dyn Error>> {
        self.close_data_stream().await;
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
