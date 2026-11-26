//! USB serial transport.
//!
//! Counterpart of ``ruida_pa.ruidadriver.transport.usb_transport``.

use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::{SerialPortBuilderExt, SerialStream};

use super::resolve_serial_device;
use crate::core::driver::DriverError;
use crate::ruida::ruidadriver::transport::base::Transport;

fn io_error(error: std::io::Error) -> DriverError {
    DriverError::Connection(error.to_string())
}

pub struct UsbTransport {
    port: SerialStream,
    buffer: Vec<u8>,
}

impl UsbTransport {
    /// Opens a USB serial link at the Ruida baud rate.
    pub fn open(device: &str) -> Result<Self, DriverError> {
        let device = resolve_serial_device(device);
        let port: SerialStream = tokio_serial::new(&device, 115_200)
            .open_native_async()
            .map_err(|error| DriverError::Connection(error.to_string()))?;
        Ok(Self {
            port,
            buffer: vec![0; 4096],
        })
    }
}

#[async_trait]
impl Transport for UsbTransport {
    async fn write(&mut self, data: &[u8]) -> Result<(), DriverError> {
        self.port.write_all(data).await.map_err(io_error)
    }

    async fn read(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<Vec<u8>>, DriverError> {
        match tokio::time::timeout(timeout, self.port.read(&mut self.buffer))
            .await
        {
            Ok(Ok(0)) => Ok(None),
            Ok(Ok(length)) => Ok(Some(self.buffer[..length].to_vec())),
            Ok(Err(error)) => Err(io_error(error)),
            Err(_) => Ok(None),
        }
    }

    async fn close(&mut self) {}

    async fn drain(&mut self) {
        while let Ok(Some(_)) = self.read(Duration::from_millis(0)).await {}
    }
}
