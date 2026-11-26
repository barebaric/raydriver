//! Raw Ruida transport base types.
//!
//! Counterpart of ``ruida_pa.ruidadriver.transport.base``.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::core::driver::{DriverError, DriverEvents};

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
pub trait Transport: Send {
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
pub struct TappedTransport {
    link: Box<dyn Transport>,
    events: Arc<dyn DriverEvents>,
}

/// Upper bound for one raw event, so a large USB job cannot create
/// an oversized event (or remote wire frame).
const RAW_EVENT_CHUNK: usize = 64 * 1024;

impl TappedTransport {
    pub fn new(
        link: Box<dyn Transport>,
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
impl Transport for TappedTransport {
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
