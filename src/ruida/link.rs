//! Raw byte links to Ruida controllers.
//!
//! This layer mirrors ``ruida_pa.ruidadriver.transport``: it only
//! opens, writes, reads and closes bytes. Packet framing, swizzling,
//! checksums, ACK handling and reply parsing belong to
//! :mod:`crate::ruida::transport`.
//!
//! USB, UDP and TCP have different wire behavior, but that difference
//! is expressed by the transport-layer ``WireProfile``; nothing here
//! inspects command payloads.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::core::driver::{DriverError, DriverEvents};

/// The physical link a session runs over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    Usb,
    Udp,
    Tcp,
}

impl TransportKind {
    pub fn is_udp(self) -> bool {
        self == Self::Udp
    }

    pub fn is_usb(self) -> bool {
        self == Self::Usb
    }
}

/// A byte link to the controller.
#[async_trait]
pub trait Link: Send {
    async fn write(&mut self, data: &[u8]) -> Result<(), DriverError>;
    async fn read(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<Vec<u8>>, DriverError>;
    async fn close(&mut self) {}
    async fn drain(&mut self) {}
}

pub(crate) fn timed_out() -> DriverError {
    DriverError::Timeout("controller did not answer".to_string())
}

/// Wraps a link and mirrors every wire chunk into raw driver events,
/// so the protocol analyzer can record binary traffic.  Chunks stay
/// swizzled: they are exactly what crossed the wire.
pub(crate) struct TappedLink {
    link: Box<dyn Link>,
    events: Arc<dyn DriverEvents>,
}

/// Upper bound for one raw event, so a large USB job cannot create
/// an oversized event (or remote wire frame).
const RAW_EVENT_CHUNK: usize = 64 * 1024;

impl TappedLink {
    pub(crate) fn new(
        link: Box<dyn Link>,
        events: Arc<dyn DriverEvents>,
    ) -> Self {
        Self { link, events }
    }

    fn report_tx(&self, data: &[u8]) {
        for chunk in data.chunks(RAW_EVENT_CHUNK) {
            self.events.raw_tx_bytes(chunk);
        }
    }

    fn report_rx(&self, data: &[u8]) {
        for chunk in data.chunks(RAW_EVENT_CHUNK) {
            self.events.raw_rx_bytes(chunk);
        }
    }
}

#[async_trait]
impl Link for TappedLink {
    async fn write(&mut self, data: &[u8]) -> Result<(), DriverError> {
        self.link.write(data).await?;
        self.report_tx(data);
        Ok(())
    }

    async fn read(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<Vec<u8>>, DriverError> {
        let data = self.link.read(timeout).await?;
        if let Some(bytes) = data.as_deref() {
            self.report_rx(bytes);
        }
        Ok(data)
    }

    async fn close(&mut self) {
        self.link.close().await;
    }

    async fn drain(&mut self) {
        self.link.drain().await;
    }
}

#[cfg(feature = "net")]
mod net {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpStream, UdpSocket};

    fn io_error(error: std::io::Error) -> DriverError {
        DriverError::Connection(error.to_string())
    }

    pub struct UdpLink {
        socket: UdpSocket,
        buffer: Vec<u8>,
    }

    impl UdpLink {
        /// Binds the host's Ruida port and connects to the controller.
        pub async fn connect(
            host: &str,
            port: u16,
        ) -> Result<Self, DriverError> {
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
    impl Link for UdpLink {
        async fn write(&mut self, data: &[u8]) -> Result<(), DriverError> {
            self.socket.send(data).await.map_err(io_error)?;
            Ok(())
        }

        async fn read(
            &mut self,
            timeout: Duration,
        ) -> Result<Option<Vec<u8>>, DriverError> {
            match tokio::time::timeout(
                timeout,
                self.socket.recv(&mut self.buffer),
            )
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

    pub struct TcpLink {
        stream: TcpStream,
        buffer: Vec<u8>,
    }

    impl TcpLink {
        /// Connects to the controller's Ruida port.
        pub async fn connect(
            host: &str,
            port: u16,
        ) -> Result<Self, DriverError> {
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
    impl Link for TcpLink {
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
            match tokio::time::timeout(
                timeout,
                self.stream.read(&mut self.buffer),
            )
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
}

#[cfg(feature = "net")]
pub use net::{TcpLink, UdpLink};

#[cfg(feature = "serial")]
mod serial {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_serial::{SerialPortBuilderExt, SerialStream};

    fn io_error(error: std::io::Error) -> DriverError {
        DriverError::Connection(error.to_string())
    }

    pub struct SerialLink {
        port: SerialStream,
        buffer: Vec<u8>,
    }

    impl SerialLink {
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
    impl Link for SerialLink {
        async fn write(&mut self, data: &[u8]) -> Result<(), DriverError> {
            self.port.write_all(data).await.map_err(io_error)
        }

        async fn read(
            &mut self,
            timeout: Duration,
        ) -> Result<Option<Vec<u8>>, DriverError> {
            match tokio::time::timeout(
                timeout,
                self.port.read(&mut self.buffer),
            )
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
}

#[cfg(feature = "serial")]
pub use serial::SerialLink;

/// Resolves ``VID:PID`` and short POSIX device names like the Python
/// transport does.
#[cfg(feature = "serial")]
fn resolve_serial_device(device: &str) -> String {
    if device.contains(':') {
        let wanted = device.to_ascii_lowercase();
        if let Ok(ports) = serialport::available_ports() {
            for port in ports {
                if let serialport::SerialPortType::UsbPort(info) =
                    port.port_type
                {
                    let vid_pid = format!("{:04x}:{:04x}", info.vid, info.pid);
                    if vid_pid == wanted || port.port_name == device {
                        return port.port_name;
                    }
                }
            }
        }
    }
    if cfg!(unix) && !device.contains('/') {
        return format!("/dev/{device}");
    }
    device.to_string()
}

/// Opens a link for the given endpoint.
pub async fn open_link(
    kind: TransportKind,
    host: Option<&str>,
    device: Option<&str>,
    port: u16,
) -> Result<Box<dyn Link>, DriverError> {
    let _ = (host, device, port);
    match kind {
        #[cfg(feature = "serial")]
        TransportKind::Usb => {
            let device = device.ok_or_else(|| {
                DriverError::Device("no USB device configured".to_string())
            })?;
            Ok(Box::new(SerialLink::open(device)?))
        }
        #[cfg(not(feature = "serial"))]
        TransportKind::Usb => Err(DriverError::Unsupported(
            "serial support is not compiled in".to_string(),
        )),
        #[cfg(feature = "net")]
        TransportKind::Udp => {
            let host = host.ok_or_else(|| {
                DriverError::Device("no controller host configured".to_string())
            })?;
            Ok(Box::new(UdpLink::connect(host, port).await?))
        }
        #[cfg(not(feature = "net"))]
        TransportKind::Udp => Err(DriverError::Unsupported(
            "network support is not compiled in".to_string(),
        )),
        #[cfg(feature = "net")]
        TransportKind::Tcp => {
            let host = host.ok_or_else(|| {
                DriverError::Device("no controller host configured".to_string())
            })?;
            Ok(Box::new(TcpLink::connect(host, port).await?))
        }
        #[cfg(not(feature = "net"))]
        TransportKind::Tcp => Err(DriverError::Unsupported(
            "network support is not compiled in".to_string(),
        )),
    }
}
