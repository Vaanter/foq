use async_trait::async_trait;
use s2n_quic::stream::BidirectionalStream;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::sync::broadcast::Receiver;

#[async_trait]
pub(crate) trait ConnectionHandler {
    async fn handle(&mut self, mut receiver: Receiver<()>) -> Result<(), anyhow::Error>;
}

pub(crate) trait AsyncReadWrite: AsyncRead + AsyncWrite + Sync + Send + Unpin {}

impl AsyncReadWrite for TcpStream {}
impl AsyncReadWrite for BidirectionalStream {}
