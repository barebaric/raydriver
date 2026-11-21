//! Telnet transport stub for builds without the `net` feature (e.g.
//! wasm).  The type keeps the canonical module path so
//! [`TransportKind::build`](super::TransportKind::build) stays
//! intact; every operation fails at runtime.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use super::{Transport, TransportError};

const UNSUPPORTED: &str =
    "telnet transport not available in this build (missing `net` feature)";

pub struct TelnetTransport {
    host: String,
    port: u16,
    open: AtomicBool,
}

impl TelnetTransport {
    pub fn new(host: &str, port: u16) -> Arc<Self> {
        Arc::new(Self {
            host: host.to_string(),
            port,
            open: AtomicBool::new(false),
        })
    }
}

#[async_trait]
impl Transport for TelnetTransport {
    async fn open(
        &self,
        _tx: mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<(), TransportError> {
        Err(TransportError::Connection(UNSUPPORTED.to_string()))
    }

    async fn close(&self) {
        self.open.store(false, Ordering::SeqCst);
    }

    async fn write(&self, _data: &[u8]) -> Result<(), TransportError> {
        Err(TransportError::Connection(UNSUPPORTED.to_string()))
    }

    fn is_open(&self) -> bool {
        self.open.load(Ordering::SeqCst)
    }

    fn resource_uri(&self) -> Option<String> {
        Some(format!("tcp://{}:{}", self.host, self.port))
    }
}
