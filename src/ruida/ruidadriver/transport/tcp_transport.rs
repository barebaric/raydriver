//! TCP transport.
//!
//! Counterpart of ``ruida_pa.ruidadriver.transport.tcp_transport``.

use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::core::driver::DriverError;
use crate::ruida::ruidadriver::transport::base::{timed_out, Transport};

fn io_error(error: std::io::Error) -> DriverError {
    DriverError::Connection(error.to_string())
}

pub struct TcpTransport {
    stream: TcpStream,
    buffer: Vec<u8>,
}

impl TcpTransport {
    /// Connects to the controller's Ruida port.
    pub async fn connect(host: &str, port: u16) -> Result<Self, DriverError> {
        let stream = tokio::time::timeout(
            Duration::from_secs(3),
            TcpStream::connect((host, port)),
        )
        .await
        .map_err(|_| timed_out())?
        .map_err(io_error)?;
        let _ = stream.set_nodelay(true);
        Ok(Self {
            stream,
            buffer: vec![0; 4096],
        })
    }
}

#[async_trait]
impl Transport for TcpTransport {
    async fn write(&mut self, data: &[u8]) -> Result<(), DriverError> {
        tokio::time::timeout(
            Duration::from_secs(2),
            self.stream.write_all(data),
        )
        .await
        .map_err(|_| timed_out())?
        .map_err(io_error)
    }

    async fn read(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<Vec<u8>>, DriverError> {
        match tokio::time::timeout(timeout, self.stream.read(&mut self.buffer))
            .await
        {
            Ok(Ok(0)) => Err(DriverError::Connection(
                "controller closed the connection".to_string(),
            )),
            Ok(Ok(length)) => Ok(Some(self.buffer[..length].to_vec())),
            Ok(Err(error)) => Err(io_error(error)),
            Err(_) => Ok(None),
        }
    }

    async fn close(&mut self) {
        let _ = self.stream.shutdown().await;
    }

    async fn drain(&mut self) {
        while let Ok(Some(_)) = self.read(Duration::from_millis(0)).await {}
    }
}
