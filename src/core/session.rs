//! Session primitives shared by the protocol sessions: the event
//! notification trait, the error type and the async latch.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::core::transport::TransportError;
use crate::core::types::{DeviceState, TransportStatus};

/// One notification from a device session.
#[derive(Debug, Clone)]
pub enum SessionEvent {
    StateChanged(DeviceState),
    ConnectionStatus {
        status: TransportStatus,
        message: Option<String>,
    },
    CommandStatus {
        status: TransportStatus,
        message: Option<String>,
    },
    JobFinished,
    JobError {
        message: String,
    },
    ProbeStatus {
        message: String,
    },
    WcsUpdated {
        offsets: HashMap<String, (f64, f64, f64)>,
    },
    ConfigChanged {
        key: String,
        value: i64,
    },
    CommandDone(i64),
    /// A complete line received from the device, including status
    /// reports, `ok`/`error:` acknowledgements and `$` replies.
    RawRx(String),
    /// A complete line sent to the device (interactive commands and
    /// realtime bytes; not bulk job streaming).
    RawTx(String),
}

/// Notifications emitted by a device session, implemented by the
/// Python bindings (or by embedders).
///
/// [`emit`](Self::emit) is the canonical entry point; the named
/// helpers below are adapters so session code stays readable.
pub trait SessionEvents: Send + Sync {
    fn emit(&self, event: SessionEvent);

    fn state_changed(&self, state: &DeviceState) {
        self.emit(SessionEvent::StateChanged(state.clone()));
    }

    fn connection_status(
        &self,
        status: TransportStatus,
        message: Option<&str>,
    ) {
        self.emit(SessionEvent::ConnectionStatus {
            status,
            message: message.map(str::to_string),
        });
    }

    fn command_status(&self, status: TransportStatus, message: Option<&str>) {
        self.emit(SessionEvent::CommandStatus {
            status,
            message: message.map(str::to_string),
        });
    }

    fn job_finished(&self) {
        self.emit(SessionEvent::JobFinished);
    }

    fn job_error(&self, message: &str) {
        self.emit(SessionEvent::JobError {
            message: message.to_string(),
        });
    }

    fn probe_status(&self, message: &str) {
        self.emit(SessionEvent::ProbeStatus {
            message: message.to_string(),
        });
    }

    fn wcs_updated(&self, offsets: &HashMap<String, (f64, f64, f64)>) {
        self.emit(SessionEvent::WcsUpdated {
            offsets: offsets.clone(),
        });
    }

    fn config_changed(&self, key: &str, value: i64) {
        self.emit(SessionEvent::ConfigChanged {
            key: key.to_string(),
            value,
        });
    }

    fn command_done(&self, op_index: i64) {
        self.emit(SessionEvent::CommandDone(op_index));
    }

    fn raw_rx(&self, line: &str) {
        self.emit(SessionEvent::RawRx(line.to_string()));
    }

    fn raw_tx(&self, line: &str) {
        self.emit(SessionEvent::RawTx(line.to_string()));
    }
}

/// No-op event sink.
pub struct NoopEvents;

impl SessionEvents for NoopEvents {
    fn emit(&self, _event: SessionEvent) {}
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
    /// The session cannot perform the operation at all (as opposed
    /// to failing at it): the protocol has no such capability, and
    /// the uniform session surface still exposes the method.
    #[error("unsupported operation: {0}")]
    Unsupported(String),
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
