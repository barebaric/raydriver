//! Raw TCP transport (telnet-style) for network-attached Grbl
//! devices.
//!
//! The open/writer/reader plumbing is shared with the serial
//! transport via [`super::stream_base`].

use std::sync::Arc;

use async_trait::async_trait;
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use super::stream_base::StreamTransportBase;
use super::{Transport, TransportError};

pub struct TelnetTransport {
    base: StreamTransportBase<TcpStream>,
    host: String,
    port: u16,
}

impl TelnetTransport {
    pub fn new(host: &str, port: u16) -> Arc<Self> {
        Arc::new(Self {
            base: StreamTransportBase::new(),
            host: host.to_string(),
            port,
        })
    }
}

#[async_trait]
impl Transport for TelnetTransport {
    async fn open(
        &self,
        tx: mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<(), TransportError> {
        let stream = TcpStream::connect((self.host.as_str(), self.port))
            .await
            .map_err(|e| TransportError::Connection(e.to_string()))?;
        self.base.start(stream, "Telnet", tx).await;
        Ok(())
    }

    async fn close(&self) {
        self.base.close().await;
    }

    async fn write(&self, data: &[u8]) -> Result<(), TransportError> {
        self.base.write(data).await
    }

    fn is_open(&self) -> bool {
        self.base.is_open()
    }

    fn resource_uri(&self) -> Option<String> {
        Some(format!("tcp://{}:{}", self.host, self.port))
    }
}
