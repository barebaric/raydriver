//! UDP transport.
//!
//! Counterpart of ``ruida_pa.ruidadriver.transport.udp_transport``.

use std::time::Duration;

use async_trait::async_trait;
use tokio::net::UdpSocket;

use crate::core::driver::DriverError;
use crate::ruida::ruidadriver::transport::base::Transport;

fn io_error(error: std::io::Error) -> DriverError {
    DriverError::Connection(error.to_string())
}

pub struct UdpTransport {
    socket: UdpSocket,
    buffer: Vec<u8>,
}

impl UdpTransport {
    /// Binds the host's Ruida port and connects to the controller.
    pub async fn connect(host: &str, port: u16) -> Result<Self, DriverError> {
        let probe = UdpSocket::bind("0.0.0.0:0").await.map_err(io_error)?;
        probe.connect((host, port)).await.map_err(io_error)?;
        let local = probe.local_addr().map_err(io_error)?;
        let socket = UdpSocket::bind((local.ip(), 40200))
            .await
            .map_err(io_error)?;
        socket.connect((host, port)).await.map_err(io_error)?;
        Ok(Self {
            socket,
            buffer: vec![0; 64 * 1024],
        })
    }
}

#[async_trait]
impl Transport for UdpTransport {
    async fn write(&mut self, data: &[u8]) -> Result<(), DriverError> {
        self.socket.send(data).await.map_err(io_error)?;
        Ok(())
    }

    async fn read(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<Vec<u8>>, DriverError> {
        match tokio::time::timeout(timeout, self.socket.recv(&mut self.buffer))
            .await
        {
            Ok(Ok(length)) => Ok(Some(self.buffer[..length].to_vec())),
            Ok(Err(error)) => {
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::Interrupted
                ) {
                    Ok(None)
                } else {
                    Err(io_error(error))
                }
            }
            Err(_) => Ok(None),
        }
    }

    async fn close(&mut self) {}

    async fn drain(&mut self) {
        while let Ok(Some(_)) = self.read(Duration::from_millis(0)).await {}
    }
}
