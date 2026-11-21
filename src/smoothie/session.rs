//! The Smoothie session: a minimal ping-pong session for
//! Smoothieware over telnet or serial.
//!
//! Port of Rayforge's `SmoothieDriver` protocol behavior.  Smoothie
//! speaks Grbl-style `<...>` status reports in response to `?`, so
//! this session deliberately reuses the GRBL status parser
//! (`grbl::parser::parse_state`) — the one sanctioned cross-protocol
//! import.  Commands use the same strict ping-pong as the minimal
//! GRBL session: send one line, wait for its `ok`/`error:`
//! acknowledgement, then send the next.
//!
//! Differences from the GRBL sessions: the connection is considered
//! established as soon as the transport answers a status poll (the
//! Smoothie welcome banner is firmware-specific and unused), and
//! cancel sends Ctrl+C instead of a soft reset.
//!
//! The ping-pong mechanics (pending acknowledgements, connection
//! loop, job streaming, cancel, safety shutdown) are shared with the
//! other line sessions via [`core::line_session`]; this module only
//! holds the Smoothie-specific dispatch and hooks.

use std::borrow::Cow;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::core::dialect::{format_template, Arg, Dialect};
use crate::core::line_session::{
    cancel, LineSessionBase, PendingStore, PingPongSession, StopCommand,
};
use crate::core::session::{secs, SessionError, SessionEvents};
use crate::core::transport::TransportKind;
use crate::core::types::{DeviceState, DeviceStatus, TransportStatus};

use crate::grbl::parser;

/// Configuration for the Smoothie session.
#[derive(Clone)]
pub struct SmoothieConfig {
    pub transport: TransportKind,
    pub poll_status_while_running: bool,
    pub command_timeout: f64,
    pub handshake_timeout: f64,
    pub handshake_poll_interval: f64,
    pub status_poll_interval: f64,
    pub reconnect_delay: f64,
    pub safety_shutdown_delay: f64,
}

impl SmoothieConfig {
    pub fn new(transport: TransportKind) -> Self {
        Self {
            transport,
            poll_status_while_running: false,
            command_timeout: 30.0,
            handshake_timeout: 15.0,
            handshake_poll_interval: 0.5,
            status_poll_interval: 1.0,
            reconnect_delay: 5.0,
            safety_shutdown_delay: 0.2,
        }
    }
}

/// The Smoothie session: connection lifecycle, status polling and
/// ping-pong command exchange.
pub struct SmoothieCore {
    base: LineSessionBase,
    config: SmoothieConfig,
}

impl SmoothieCore {
    pub fn new(
        config: SmoothieConfig,
        dialect: Dialect,
        events: Arc<dyn SessionEvents>,
    ) -> Arc<Self> {
        let transport = config.transport.build();
        Arc::new(Self {
            base: LineSessionBase::new(
                dialect,
                events,
                transport,
                PendingStore::slot(),
            ),
            config,
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
    /// `Drop` where blocking is not allowed.  A session dropped
    /// without `disconnect()` must not leave its connection, polling
    /// and reader tasks running on the shared runtime: such orphans
    /// accumulate across sessions and starve the worker threads
    /// (hangs on Linux, access violations on Windows).
    pub fn abort_background_tasks_now(&self) {
        self.base.abort_background_tasks_now();
    }

    // ---------------------------------------------------------------
    // Cancel
    // ---------------------------------------------------------------

    /// Abort a running job.  Smoothieware stops on Ctrl+C.
    pub async fn cancel(&self, emergency: bool) -> Result<(), SessionError> {
        cancel(
            self,
            emergency,
            StopCommand {
                bytes: b"\x03",
                log: "Sending Ctrl+C to device.",
            },
            "Transport not connected",
        )
        .await
    }

    // ---------------------------------------------------------------
    // Handshake / polling
    // ---------------------------------------------------------------

    /// Poll the device with `?` until a status report arrives —
    /// Smoothieware needs a moment after opening the link before it
    /// accepts commands, and silently drops earlier queries.
    async fn await_handshake(&self) -> bool {
        let deadline =
            tokio::time::Instant::now() + secs(self.config.handshake_timeout);
        loop {
            self.base.events.raw_tx("?");
            if let Err(err) = self.base.transport.write(b"?").await {
                log::error!("Connection error during handshake: {err}");
                return false;
            }
            let poll_deadline = tokio::time::Instant::now()
                + secs(self.config.handshake_poll_interval);
            tokio::select! {
                _ = self.base.handshake_received.wait() => return true,
                _ = tokio::time::sleep_until(poll_deadline) => {
                    if tokio::time::Instant::now() >= deadline {
                        return false;
                    }
                }
            }
        }
    }

    async fn status_poll_loop(&self) {
        while self.base.transport.is_open()
            && self.base.keep_running.load(Ordering::SeqCst)
        {
            if self.config.poll_status_while_running
                || !self.base.job_running.load(Ordering::SeqCst)
            {
                let _guard = self.base.cmd_lock.lock().await;
                if self.base.transport.is_open() {
                    self.base.events.raw_tx("?");
                    if let Err(err) = self.base.transport.write(b"?").await {
                        log::warn!("Connection lost while polling: {err}");
                        break;
                    }
                }
            }
            tokio::time::sleep(secs(self.config.status_poll_interval)).await;
        }
    }

    // ---------------------------------------------------------------
    // RX handling
    // ---------------------------------------------------------------

    fn handle_error_line(&self, line: &str) {
        let code = line.split(':').nth(1).unwrap_or("").trim();
        let error = parser::get_error(code);
        self.base.set_device_error(error);
        self.base.pending.finish_next(Some(line));

        if self.base.is_job_running() {
            log::error!("Device error during job: {line}. Halting stream.");
            *self.base.job_exception.lock().unwrap() =
                Some(SessionError::Device(format!("Device error: {line}")));
        }
    }

    fn handle_alarm(&self, code: &str, line: &str) {
        let error = crate::grbl::errors::alarm_code_to_device_error(code);
        let snapshot = {
            let mut state = self.base.state.lock().unwrap();
            if state.error.is_none() {
                state.error = Some(error);
            }
            state.status = DeviceStatus::Alarm;
            Some(state.clone())
        };
        if let Some(snapshot) = snapshot {
            self.base.events.state_changed(&snapshot);
        }
        self.base.pending.finish_next(Some(line));

        if self.base.is_job_running() {
            log::error!("Device ALARM during job: {line}. Halting stream.");
            *self.base.job_exception.lock().unwrap() =
                Some(SessionError::Device(format!("Device ALARM: {line}")));
        }
    }

    /// Parse a Grbl-style status report (Smoothie emits them for
    /// `?`; see the module docs for why the GRBL parser is reused).
    fn handle_status_report(&self, report: &str) {
        self.base.handshake_received.set();
        let mut state = {
            let current = self.base.state.lock().unwrap();
            parser::parse_state(report, &current, false)
        };

        // While a job runs, 'Idle' between commands is reported as
        // 'Run'; a requested hold owns the pause state.
        if self.base.is_job_running() && state.status == DeviceStatus::Idle {
            state.status = DeviceStatus::Run;
        }
        if self.base.is_holding.load(Ordering::SeqCst)
            && state.status != DeviceStatus::Alarm
        {
            state.status = DeviceStatus::Hold;
        }

        self.base.emit_state_if_changed(state);
    }

    // ---------------------------------------------------------------
    // Hold / device operations
    // ---------------------------------------------------------------

    pub async fn set_hold(&self, hold: bool) -> Result<(), SessionError> {
        self.base.is_holding.store(hold, Ordering::SeqCst);
        if !self.base.transport.is_open() {
            return Err(SessionError::Connection(
                "Transport not connected".to_string(),
            ));
        }
        self.base
            .transport
            .write(if hold { b"!" } else { b"~" })
            .await?;
        let desired = if hold {
            DeviceStatus::Hold
        } else if self.base.is_job_running() {
            DeviceStatus::Run
        } else {
            DeviceStatus::Idle
        };
        let snapshot = {
            let mut state = self.base.state.lock().unwrap();
            if state.status != desired {
                state.status = desired;
                Some(state.clone())
            } else {
                None
            }
        };
        if let Some(snapshot) = snapshot {
            self.base.events.state_changed(&snapshot);
        }
        Ok(())
    }

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
        let has_error = response.iter().any(|line| line.starts_with("error:"));
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
        let p_num = parser::gcode_to_p_number(wcs_slot).ok_or_else(|| {
            SessionError::Other(format!("Invalid WCS slot: {wcs_slot}"))
        })?;
        let dialect = self.base.dialect();
        let cmd = dialect.format_wcs_offset(p_num, x, y, z);
        self.execute_command(cmd).await?;
        Ok(())
    }
}

#[async_trait]
impl PingPongSession for SmoothieCore {
    fn base(&self) -> &LineSessionBase {
        &self.base
    }

    fn handle_line(&self, line: &str) {
        if line == "ok" {
            self.base.events.raw_rx("ok");
            self.base.pending.finish_next(Some("ok"));
            return;
        }
        if line.starts_with("error:") {
            self.base.events.raw_rx(line);
            self.handle_error_line(line);
            return;
        }
        if line.starts_with('<') && line.ends_with('>') {
            self.base.events.raw_rx(line);
            self.handle_status_report(line);
            return;
        }
        if let Some(code) = line.strip_prefix("ALARM:") {
            self.base.events.raw_rx(line);
            self.handle_alarm(code.trim(), line);
            return;
        }
        self.base.events.raw_rx(line);
        self.base.pending.collect(line);
    }

    async fn handshake(&self) -> bool {
        self.await_handshake().await
    }

    async fn poll_loop(&self) {
        self.status_poll_loop().await;
    }

    fn on_handshake_failed(&self) {
        log::warn!(
            "No response from device. Port may be a phantom connection."
        );
        self.base.connection_status(
            TransportStatus::Error,
            Some("No response from device"),
        );
    }

    async fn ping_pong(
        &self,
        command: &str,
    ) -> Result<Vec<String>, SessionError> {
        let _guard = self.base.cmd_lock.lock().await;
        if !self.base.transport.is_open() {
            return Err(SessionError::Connection(
                "Transport not connected".to_string(),
            ));
        }
        self.base
            .await_ack(command, secs(self.config.command_timeout))
            .await
    }

    fn job_log_label(&self) -> &'static str {
        "Smoothie"
    }

    fn prep_job_line<'a>(&'a self, raw: &'a str) -> Cow<'a, str> {
        Cow::Owned(parser::strip_gcode_comments(raw))
    }

    fn safety_shutdown_delay(&self) -> Option<Duration> {
        Some(secs(self.config.safety_shutdown_delay))
    }

    fn reconnect_delay(&self) -> Duration {
        secs(self.config.reconnect_delay)
    }
}
