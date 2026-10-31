//! Session primitives shared by the protocol sessions: the event
//! notification trait, the error type and the async latch.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::core::transport::TransportError;
use crate::core::types::{DeviceState, TransportStatus};

/// Notifications emitted by a device session, implemented by the
/// Python bindings (or by embedders).
pub trait SessionEvents: Send + Sync {
    fn state_changed(&self, state: &DeviceState);
    fn connection_status(&self, status: TransportStatus, message: Option<&str>);
    fn command_status(&self, status: TransportStatus, message: Option<&str>);
    fn job_finished(&self);
    fn probe_status(&self, message: &str);
    fn wcs_updated(&self, offsets: &HashMap<String, (f64, f64, f64)>);
    fn config_changed(&self, key: &str, value: i64);
    fn command_done(&self, op_index: i64);

    /// A complete line received from the device, including status
    /// reports, `ok`/`error:` acknowledgements and `$` replies.
    fn raw_rx(&self, _line: &str) {}

    /// A complete line sent to the device (interactive commands and
    /// realtime bytes; not bulk job streaming).
    fn raw_tx(&self, _line: &str) {}
}

/// No-op event sink.
pub struct NoopEvents;

impl SessionEvents for NoopEvents {
    fn state_changed(&self, _state: &DeviceState) {}
    fn connection_status(
        &self,
        _status: TransportStatus,
        _message: Option<&str>,
    ) {
    }
    fn command_status(&self, _status: TransportStatus, _message: Option<&str>) {
    }
    fn job_finished(&self) {}
    fn probe_status(&self, _message: &str) {}
    fn wcs_updated(&self, _offsets: &HashMap<String, (f64, f64, f64)>) {}
    fn config_changed(&self, _key: &str, _value: i64) {}
    fn command_done(&self, _op_index: i64) {}
}

#[derive(thiserror::Error, Debug)]
pub enum SessionError {
    #[error("{0}")]
    Connection(String),
    #[error("operation timed out")]
    Timeout,
    #[error("{0}")]
    Device(String),
    #[error("buffer stall: {0}")]
    BufferStall(String),
    #[error("{0}")]
    Other(String),
}

impl From<TransportError> for SessionError {
    fn from(err: TransportError) -> Self {
        match err {
            TransportError::Connection(msg) => Self::Connection(msg),
            TransportError::NotConnected => {
                Self::Connection("not connected".to_string())
            }
        }
    }
}

/// An asyncio.Event-style latch built on tokio primitives.
pub(crate) struct AsyncEvent {
    set: Mutex<bool>,
    notify: tokio::sync::Notify,
}

impl AsyncEvent {
    pub(crate) fn new() -> Self {
        Self {
            set: Mutex::new(false),
            notify: tokio::sync::Notify::new(),
        }
    }

    pub(crate) fn set(&self) {
        *self.set.lock().unwrap() = true;
        self.notify.notify_waiters();
        self.notify.notify_one();
    }

    pub(crate) fn clear(&self) {
        *self.set.lock().unwrap() = false;
    }

    pub(crate) fn is_set(&self) -> bool {
        *self.set.lock().unwrap()
    }

    pub(crate) async fn wait(&self) {
        loop {
            if self.is_set() {
                return;
            }
            let notified = self.notify.notified();
            if self.is_set() {
                return;
            }
            notified.await;
        }
    }
}
