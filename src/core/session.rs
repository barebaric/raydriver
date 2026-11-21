//! Session primitives shared by the protocol sessions: the event
//! notification trait, the error type, the async latch and small
//! shared helpers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;

use crate::core::transport::TransportError;
use crate::core::types::{DeviceState, TransportStatus, UnitSystem};

/// Converts a (possibly negative) seconds value into a non-negative
/// [`Duration`]; shared by every session's timeout configuration.
pub(crate) fn secs(v: f64) -> Duration {
    Duration::from_secs_f64(v.max(0.0))
}

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

/// The canonical error for a [`Session`] operation the protocol
/// cannot perform at all.
pub fn unsupported(op: &str) -> SessionError {
    SessionError::Unsupported(format!("{op} is not supported by this protocol"))
}

/// Cancel urgency for [`Session::cancel`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelMode {
    /// Cooperative stop; the device may finish the current move.
    Graceful,
    /// Immediate stop (quick stop, feed hold or soft reset).
    Immediate,
}

/// The uniform, typed session surface shared by every protocol
/// driver.
///
/// This is the canonical convention the protocol sessions used to
/// follow only by name — `run`/`cancel`/`home` signatures used to
/// drift per protocol.  The native session methods remain the
/// behavior source; these impls adapt them (and their historical
/// signature differences) losslessly.  Operations a protocol cannot
/// perform at all return [`SessionError::Unsupported`].
#[async_trait]
pub trait Session: Send + Sync {
    /// Open the connection.  Connection problems are reported
    /// through [`SessionEvents::connection_status`], not the return
    /// value.
    async fn connect(self: Arc<Self>) -> Result<(), SessionError>;

    /// Close the connection and stop all background tasks.
    async fn cleanup(&self);

    /// Stream (or upload) a complete G-code job and wait for it to
    /// finish.  *op_map* maps zero-based G-code line indices to op
    /// indices; *op_estimates* carries the per-op durations in
    /// seconds where the protocol can use them.
    async fn run(
        self: Arc<Self>,
        gcode: &str,
        op_map: &HashMap<usize, i64>,
        op_estimates: &[f64],
    ) -> Result<(), SessionError> {
        let _ = (gcode, op_map, op_estimates);
        Err(unsupported("run"))
    }

    /// Stop the running job.
    async fn cancel(&self, mode: CancelMode) -> Result<(), SessionError>;

    /// Send one interactive command and await its full response.
    async fn execute_interactive_command(
        &self,
        command: &str,
    ) -> Result<Vec<String>, SessionError> {
        let _ = command;
        Err(unsupported("execute_interactive_command"))
    }

    /// Home all axes or *axes*; *active_wcs* is the WCS the caller
    /// expects to stay active (some protocols re-assert it).
    async fn home(
        &self,
        axes: Option<Vec<String>>,
        active_wcs: &str,
    ) -> Result<(), SessionError> {
        let _ = (axes, active_wcs);
        Err(unsupported("home"))
    }

    /// Absolute move in the machine's current coordinate frame.
    async fn move_to(
        &self,
        speed: f64,
        x: f64,
        y: f64,
        z: Option<f64>,
    ) -> Result<(), SessionError> {
        let _ = (speed, x, y, z);
        Err(unsupported("move_to"))
    }

    /// Relative jog by per-axis distances.
    async fn jog(
        &self,
        speed: f64,
        deltas: &[(String, f64)],
    ) -> Result<(), SessionError> {
        let _ = (speed, deltas);
        Err(unsupported("jog"))
    }

    async fn select_tool(&self, tool_number: i64) -> Result<(), SessionError> {
        let _ = tool_number;
        Err(unsupported("select_tool"))
    }

    /// Laser/spindle power; `None` disables the tool.
    async fn set_power(&self, power: Option<f64>) -> Result<(), SessionError> {
        let _ = power;
        Err(unsupported("set_power"))
    }

    /// Laser power used while focusing; `None` disables it.
    async fn set_focus_power(
        &self,
        power: Option<f64>,
    ) -> Result<(), SessionError> {
        let _ = power;
        Err(unsupported("set_focus_power"))
    }

    /// Pause or resume the running job.
    async fn set_hold(&self, hold: bool) -> Result<(), SessionError> {
        let _ = hold;
        Err(unsupported("set_hold"))
    }

    async fn clear_alarm(&self) -> Result<(), SessionError> {
        Err(unsupported("clear_alarm"))
    }

    async fn set_wcs_offset(
        &self,
        wcs_slot: &str,
        x: f64,
        y: f64,
        z: Option<f64>,
    ) -> Result<(), SessionError> {
        let _ = (wcs_slot, x, y, z);
        Err(unsupported("set_wcs_offset"))
    }

    async fn read_settings(
        &self,
    ) -> Result<Vec<(String, String)>, SessionError> {
        Err(unsupported("read_settings"))
    }

    async fn write_setting(
        &self,
        key: &str,
        value: &str,
    ) -> Result<(), SessionError> {
        let _ = (key, value);
        Err(unsupported("write_setting"))
    }

    async fn read_wcs_offsets(
        &self,
    ) -> Result<HashMap<String, (f64, f64, f64)>, SessionError> {
        Err(unsupported("read_wcs_offsets"))
    }

    async fn read_parser_state(&self) -> Result<Option<String>, SessionError> {
        Err(unsupported("read_parser_state"))
    }

    /// Probe along *axis_letter*; `Ok(None)` = no trigger reported.
    async fn run_probe_cycle(
        &self,
        axis_letter: &str,
        max_travel: f64,
        feed_rate: f64,
    ) -> Result<Option<(f64, f64, f64)>, SessionError> {
        let _ = (axis_letter, max_travel, feed_rate);
        Err(unsupported("run_probe_cycle"))
    }

    async fn detect_unit_system(
        &self,
    ) -> Result<Option<UnitSystem>, SessionError> {
        Err(unsupported("detect_unit_system"))
    }
}

/// An asyncio.Event-style latch built on tokio primitives: the
/// `handshake` latch of the line sessions and generally useful for
/// session implementations.
pub struct AsyncEvent {
    set: Mutex<bool>,
    notify: tokio::sync::Notify,
}

impl Default for AsyncEvent {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncEvent {
    pub fn new() -> Self {
        Self {
            set: Mutex::new(false),
            notify: tokio::sync::Notify::new(),
        }
    }

    pub fn set(&self) {
        *self.set.lock().unwrap() = true;
        self.notify.notify_waiters();
        self.notify.notify_one();
    }

    pub fn clear(&self) {
        *self.set.lock().unwrap() = false;
    }

    pub fn is_set(&self) -> bool {
        *self.set.lock().unwrap()
    }

    pub async fn wait(&self) {
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
