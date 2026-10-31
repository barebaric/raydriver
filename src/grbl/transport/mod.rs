//! Byte transports for talking to Grbl devices.

pub mod mock;
pub mod serial;
pub mod telnet;

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::grbl::transport::mock::MockInner;
use crate::grbl::transport::serial::SerialTransport;
use crate::grbl::transport::telnet::TelnetTransport;
use crate::grbl::types::TransportStatus;

/// A connection-orientiated byte transport to a device.
///
/// Implementations push received bytes into the channel handed to
/// [`Transport::open`] — the session runs the receive loop.  Writes
/// must be safe to call concurrently with reads.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Open the connection.  Received bytes are delivered to `tx`
    /// until the connection is closed.  Errors map to
    /// [`TransportError::Connection`].
    async fn open(
        &self,
        tx: mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<(), TransportError>;

    /// Close the connection, stopping any receive task.
    async fn close(&self);

    /// Write bytes to the device.
    async fn write(&self, data: &[u8]) -> Result<(), TransportError>;

    /// True while the connection is open.
    fn is_open(&self) -> bool;

    /// Human-readable resource identifier for this transport, e.g.
    /// `serial:///dev/ttyUSB0`.
    fn resource_uri(&self) -> Option<String> {
        None
    }

    /// Transport-level status hint after a failed operation.
    fn status_on_error(&self) -> TransportStatus {
        TransportStatus::Error
    }
}

/// Which physical transport to use.
///
/// This is the transport registry: adding a transport means adding a
/// variant plus a [`TransportKind::build`] arm — sessions and the
/// Python config parsing stay untouched.
#[derive(Clone)]
pub enum TransportKind {
    Serial { port: String, baudrate: u32 },
    Telnet { host: String, port: u16 },
    Mock(Arc<MockInner>),
}

impl TransportKind {
    /// Construct the transport implementation for this kind.
    pub fn build(&self) -> Arc<dyn Transport> {
        match self {
            TransportKind::Serial { port, baudrate } => {
                SerialTransport::new(port, *baudrate)
            }
            TransportKind::Telnet { host, port } => {
                TelnetTransport::new(host, *port)
            }
            TransportKind::Mock(inner) => inner.clone(),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum TransportError {
    #[error("connection error: {0}")]
    Connection(String),
    #[error("not connected")]
    NotConnected,
}

impl From<std::io::Error> for TransportError {
    fn from(err: std::io::Error) -> Self {
        Self::Connection(err.to_string())
    }
}
