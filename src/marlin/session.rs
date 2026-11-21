//! The Marlin session: a minimal ping-pong session for Marlin
//! firmware over serial.
//!
//! Port of Rayforge's `MarlinSerialDriver` protocol behavior.  The
//! device announces itself by sending output unprompted — stock
//! Marlin prints a `start` line, but custom forks may emit only a
//! boot banner or ack with bare `ok`; a port that stays silent
//! marks a phantom.  Positions are polled with `M114` instead of `?`
//! status reports, and there is no feed-hold support — pausing is
//! the caller's concern.  Commands use the same strict ping-pong as
//! the other minimal sessions: send one line, wait for its
//! `ok`/`error:` acknowledgement, then send the next.
//!
//! Unlike the Python driver this was ported from, boot and `echo:`
//! lines are collected as response lines of the pending command, so
//! M115/M149 probes actually receive them.
//!
//! Positions are reported in device units, unconverted; unit
//! conversion stays with the caller (see
//! [`parser::detect_unit_system_from_m149`]).
//!
//! The ping-pong mechanics (pending acknowledgements, connection
//! loop, job streaming, cancel, safety shutdown) are shared with the
//! other line sessions via [`core::line_session`]; this module only
//! holds the Marlin-specific dispatch, the wire-ordered FIFO
//! pending queue and the M114 position polling.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::core::dialect::{format_template, Arg, Dialect};
use crate::core::line_session::{
    cancel, LineSessionBase, PendingStore, PingPongSession, StopCommand,
};
use crate::core::session::{secs, AsyncEvent, SessionError, SessionEvents};
use crate::core::transport::TransportKind;
use crate::core::types::{
    DeviceState, DeviceStatus, TransportStatus, UnitSystem,
};

use super::parser;

/// Configuration for the Marlin session.
#[derive(Clone)]
pub struct MarlinConfig {
    pub transport: TransportKind,
    pub poll_status_while_running: bool,
    pub command_timeout: f64,
    pub handshake_timeout: f64,
    pub handshake_poll_interval: f64,
    pub status_poll_interval: f64,
    pub reconnect_delay: f64,
    /// Marlin needs a moment after `start` before it accepts
    /// commands reliably.
    pub connect_settle: f64,
}

impl MarlinConfig {
    pub fn new(transport: TransportKind) -> Self {
        Self {
            transport,
            poll_status_while_running: false,
            command_timeout: 30.0,
            handshake_timeout: 5.0,
            handshake_poll_interval: 0.5,
            status_poll_interval: 2.0,
            reconnect_delay: 5.0,
            connect_settle: 1.0,
        }
    }
}

/// The Marlin session: connection lifecycle, M114 position polling
/// and ping-pong command exchange.
pub struct MarlinCore {
    base: LineSessionBase,
    config: MarlinConfig,
    /// Set once the device announced itself; commands must not be
    /// sent before that: the boot banner's trailing ok would
    /// otherwise resolve a pending command.
    connected: AsyncEvent,
}

impl MarlinCore {
    pub fn new(
        config: MarlinConfig,
        dialect: Dialect,
        events: Arc<dyn SessionEvents>,
    ) -> Arc<Self> {
        let transport = config.transport.build();
        Arc::new(Self {
            base: LineSessionBase::new(
                dialect,
                events,
                transport,
                PendingStore::queue(),
            ),
            config,
            connected: AsyncEvent::new(),
        })
    }

    // ---------------------------------------------------------------
    // Accessors
    // ---------------------------------------------------------------

    pub fn state(&self) -> DeviceState {
        self.base.state()
    }

    pub fn is_job_running(&self) -> bool {
        self.base.is_job_running()
    }

    pub fn resource_uri(&self) -> Option<String> {
        self.base.resource_uri()
    }

    pub fn update_dialect(&self, dialect: Dialect) {
        self.base.update_dialect(dialect);
    }

    // ---------------------------------------------------------------
    // Connect / disconnect
    // ---------------------------------------------------------------

    pub async fn connect(self: &Arc<Self>) {
        LineSessionBase::connect(self).await;
    }

    pub async fn cleanup(&self) {
        self.base.cleanup().await;
    }

    /// Abort every background task without awaiting, for use from
    /// `Drop`.  A session dropped without `disconnect()` must not
    /// leave its tasks running on the shared runtime: the orphans
    /// keep marshaling events onto an event loop that may already be
    /// closed (access violations on Windows).
    pub fn abort_background_tasks_now(&self) {
        self.base.abort_background_tasks_now();
    }

    // ---------------------------------------------------------------
    // Handshake / polling
    // ---------------------------------------------------------------

    /// Wait for the device to announce itself.  Booting devices
    /// speak unprompted — with `start`, a banner or an ack — so
    /// this simply waits for the first output line.
    async fn await_handshake(&self) -> bool {
        let deadline =
            tokio::time::Instant::now() + secs(self.config.handshake_timeout);
        tokio::select! {
            _ = self.base.handshake_received.wait() => true,
            _ = tokio::time::sleep_until(deadline) => false,
        }
    }

    async fn position_poll_loop(&self) {
        loop {
            // Poll on an interval, counting from the connection: a
            // freshly booted device gets a moment before the first
            // M114.
            tokio::time::sleep(secs(self.config.status_poll_interval)).await;
            if !self.base.transport.is_open()
                || !self.base.keep_running.load(Ordering::SeqCst)
            {
                break;
            }
            if self.config.poll_status_while_running
                || !self.base.job_running.load(Ordering::SeqCst)
            {
                // A tracked exchange, not fire-and-forget: M114's ok
                // must not resolve a later command's pending
                // acknowledgement.  The FIFO pending queue orders the
                // responses; no cmd_lock is taken so a slow device
                // cannot stall interactive commands.
                if let Err(err) = self.enqueue_and_wait("M114", false).await {
                    log::warn!("Connection lost while polling: {err}");
                    break;
                }
            }
        }
    }

    // ---------------------------------------------------------------
    // RX handling
    // ---------------------------------------------------------------

    /// Accepts the first device output as the handshake.  The
    /// session transmits nothing until the handshake resolves, so
    /// any complete line here is unprompted device output: proof of
    /// a live device.  Stock Marlin announces itself with `start`,
    /// but custom firmware forks may only print a banner or ack
    /// with bare `ok`; requiring the exact line locked those out.
    /// Only a fully silent port is a phantom.
    fn maybe_announce_handshake(&self, line: &str) {
        if self.base.handshake_received.is_set() {
            return;
        }
        if line.trim() != "start" {
            log::debug!("Handshake satisfied by unsolicited output: {line:?}");
        }
        self.base.handshake_received.set();
    }

    fn handle_error_line(&self, line: &str) {
        let message = parser::parse_error_message(line);
        let snapshot = {
            let mut state = self.base.state.lock().unwrap();
            if state.error.is_none() {
                state.error = Some(crate::core::types::DeviceError::new(
                    -1,
                    "Device Error",
                    &message,
                ));
                Some(state.clone())
            } else {
                None
            }
        };
        if let Some(snapshot) = snapshot {
            self.base.events.state_changed(&snapshot);
        }
        self.base.pending.finish_next(Some(line));

        if self.base.is_job_running() {
            log::error!("Device error during job: {line}. Halting stream.");
            *self.base.job_exception.lock().unwrap() =
                Some(SessionError::Device(format!("Device error: {line}")));
        }
    }

    fn handle_position_report(&self, line: &str) {
        let Some((x, y, z)) = parser::parse_m114_line(line) else {
            return;
        };
        let snapshot = {
            let mut state = self.base.state.lock().unwrap();
            state.machine_pos = vec![Some(x), Some(y), Some(z)];
            state.work_pos = vec![Some(x), Some(y), Some(z)];
            if state.status == DeviceStatus::Unknown {
                state.status = DeviceStatus::Idle;
            }
            Some(state.clone())
        };
        if let Some(snapshot) = snapshot {
            self.base.events.state_changed(&snapshot);
        }
    }

    // ---------------------------------------------------------------
    // Command exchange
    // ---------------------------------------------------------------

    /// Enqueue *command* and await its acknowledgement.  Outstanding
    /// commands form a FIFO queue matching the device's wire-ordered
    /// responses, so a status poll may be in flight while a user
    /// command waits without either stealing the other's
    /// acknowledgement.
    ///
    /// With `serialize` the write is excluded against other user
    /// commands; the status poll passes `false` so a slow device
    /// cannot stall interactive commands while its own response is
    /// pending.
    async fn enqueue_and_wait(
        &self,
        command: &str,
        serialize: bool,
    ) -> Result<Vec<String>, SessionError> {
        let _guard = if serialize {
            Some(self.base.cmd_lock.lock().await)
        } else {
            None
        };
        if !self.base.transport.is_open() {
            return Err(SessionError::Connection(
                "Serial transport not connected".to_string(),
            ));
        }
        self.base
            .await_ack(command, secs(self.config.command_timeout))
            .await
    }

    // ---------------------------------------------------------------
    // Device operations
    // ---------------------------------------------------------------

    pub async fn home(
        &self,
        axes: Option<Vec<String>>,
    ) -> Result<(), SessionError> {
        let dialect = self.base.dialect();
        match axes {
            None => {
                self.execute_command(dialect.home_all.clone()).await?;
            }
            Some(axes) => {
                for axis in axes {
                    let cmd = format_template(
                        &dialect.home_axis,
                        &[("axis_letter", Arg::Str(axis))],
                    )
                    .map_err(SessionError::Other)?;
                    self.execute_command(cmd).await?;
                }
            }
        }
        self.base.clear_device_error();
        Ok(())
    }

    pub async fn clear_alarm(&self) -> Result<(), SessionError> {
        let dialect = self.base.dialect();
        let response =
            self.execute_command(dialect.clear_alarm.clone()).await?;
        let has_error = response.iter().any(|line| parser::is_error(line));
        if !has_error {
            self.base.clear_device_error();
        }
        Ok(())
    }

    pub async fn set_wcs_offset(
        &self,
        wcs_slot: &str,
        x: f64,
        y: f64,
        z: Option<f64>,
    ) -> Result<(), SessionError> {
        let p_num = crate::grbl::parser::gcode_to_p_number(wcs_slot)
            .ok_or_else(|| {
                SessionError::Other(format!("Invalid WCS slot: {wcs_slot}"))
            })?;
        let dialect = self.base.dialect();
        let cmd = dialect.format_wcs_offset(p_num, x, y, z);
        self.execute_command(cmd).await?;
        Ok(())
    }

    pub async fn detect_unit_system(
        &self,
    ) -> Result<Option<UnitSystem>, SessionError> {
        let lines =
            match self.execute_interactive_command("M149".to_string()).await {
                Ok(lines) => lines,
                Err(err) => {
                    log::warn!("Unit system detection failed: {err}");
                    return Ok(None);
                }
            };
        Ok(parser::detect_unit_system_from_m149(&lines))
    }
}

#[async_trait]
impl PingPongSession for MarlinCore {
    fn base(&self) -> &LineSessionBase {
        &self.base
    }

    fn handle_line(&self, line: &str) {
        self.maybe_announce_handshake(line);
        if parser::is_ok(line) {
            self.base.events.raw_rx(line);
            self.base.pending.finish_next(Some(line));
            return;
        }
        if parser::is_error(line) {
            self.base.events.raw_rx(line);
            self.handle_error_line(line);
            return;
        }
        if parser::is_m114_position(line) {
            self.base.events.raw_rx(line);
            self.handle_position_report(line);
            self.base.pending.collect(line);
            return;
        }
        self.base.events.raw_rx(line);
        self.base.pending.collect(line);
    }

    async fn handshake(&self) -> bool {
        self.await_handshake().await
    }

    async fn on_established(&self) {
        // The device needs a moment after announcing itself.
        tokio::time::sleep(secs(self.config.connect_settle)).await;
        // Commands must not be sent before the device announced
        // itself: the boot banner's trailing ok would otherwise
        // resolve a pending command.
        self.connected.set();
        self.base
            .connection_status(TransportStatus::Connected, None);
    }

    fn on_handshake_failed(&self) {
        log::warn!(
            "Device sent no output. Port may be a phantom COM port \
             without a connected device."
        );
        self.base.connection_status(
            TransportStatus::Error,
            Some("No response from device"),
        );
    }

    async fn poll_loop(&self) {
        self.position_poll_loop().await;
    }

    async fn ping_pong(
        &self,
        command: &str,
    ) -> Result<Vec<String>, SessionError> {
        if !self.connected.is_set() {
            // Wait out the handshake: sending earlier would let the
            // boot banner's trailing ok resolve this command.
            let wait_deadline = tokio::time::Instant::now()
                + secs(
                    self.config.handshake_timeout
                        + self.config.connect_settle
                        + 2.0,
                );
            if tokio::time::timeout_at(wait_deadline, self.connected.wait())
                .await
                .is_err()
            {
                return Err(SessionError::Connection(
                    "device did not announce itself".to_string(),
                ));
            }
        }
        if !self.base.transport.is_open() {
            return Err(SessionError::Connection(
                "Serial transport not connected".to_string(),
            ));
        }
        self.enqueue_and_wait(command, true).await
    }

    fn job_log_label(&self) -> &'static str {
        "Marlin"
    }

    fn stop_job_on_alarm(&self) -> bool {
        false
    }

    fn safety_shutdown_delay(&self) -> Option<Duration> {
        None
    }

    fn reconnect_delay(&self) -> Duration {
        secs(self.config.reconnect_delay)
    }
}

/// Abort a running job by sending `M410` (Quick Stop).
impl MarlinCore {
    pub async fn cancel(&self, emergency: bool) -> Result<(), SessionError> {
        cancel(
            self,
            emergency,
            StopCommand {
                bytes: b"M410\n",
                log: "Sending M410 (Quick Stop) to device.",
            },
            "Serial transport not connected",
        )
        .await
    }
}
