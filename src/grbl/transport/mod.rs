//! Byte transports for talking to Grbl devices.

pub mod mock;
pub mod serial;
pub mod telnet;

use async_trait::async_trait;
use tokio::sync::mpsc;

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
