//! A deliberately minimal GRBL session: ping-pong only.
//!
//! Port of Rayforge's `GrblSerialSimpleDriver`: send one line, wait
//! for its `ok`/`error:` acknowledgement, then send the next.  There
//! is no character-counting flow control, no stall detection, no
//! deadlock recovery and no buffer management — the simplicity is
//! the point: this session is for hardware where the full driver
//! misbehaves, and it must not inherit the full driver's complexity.
//!
//! It shares the ping-pong mechanics (pending acknowledgements,
//! connection loop, job streaming, cancel, safety shutdown) with the
//! other line sessions via [`core::line_session`], and the leaf
//! building blocks with the full session: transports, the parser,
//! the dialect templates, the value types and the [`SessionEvents`]
//! notification trait.  Behavioural contract difference: interactive
//! commands return an `error:` line as their last response line
//! instead of raising (the raydriver contract), while an error
//! response during a job halts the stream and triggers the safety
//! shutdown.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use super::parser;
use crate::core::dialect::{format_template, Arg, Dialect};
use crate::core::line_session::{
    cancel, LineSessionBase, PendingStore, PingPongSession, StopCommand,
};
use crate::core::session::{
    secs, CancelMode, Session, SessionError, SessionEvents,
};
use crate::core::transport::TransportKind;
use crate::core::types::{DeviceStatus, TransportStatus, UnitSystem};

/// Configuration for the simple ping-pong session.
#[derive(Clone)]
pub struct SimpleConfig {
    pub transport: TransportKind,
    pub poll_status_while_running: bool,
    pub command_timeout: f64,
    pub handshake_timeout: f64,
    pub handshake_poll_interval: f64,
    pub status_poll_interval: f64,
    pub reconnect_delay: f64,
    pub safety_shutdown_delay: f64,
}

impl SimpleConfig {
    pub fn new(transport: TransportKind) -> Self {
        Self {
            transport,
            poll_status_while_running: false,
            command_timeout: 30.0,
            handshake_timeout: 3.0,
            handshake_poll_interval: 0.5,
            status_poll_interval: 1.0,
            reconnect_delay: 5.0,
            safety_shutdown_delay: 0.2,
        }
    }
}

/// The simple session: connection lifecycle, status polling and
/// ping-pong command exchange, all in one small state machine.
pub struct SimpleCore {
    base: LineSessionBase,
    config: SimpleConfig,
    report_in_inches: AtomicBool,
}

impl SimpleCore {
    pub fn new(
        config: SimpleConfig,
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
            report_in_inches: AtomicBool::new(false),
        })
    }

    // ---------------------------------------------------------------
    // Handshake / polling
    // ---------------------------------------------------------------

    /// Poll the device with `?` until it answers, bounded by the
    /// handshake timeout.  Devices that are still booting silently
    /// drop queries sent before their serial stream is ready.
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

    fn handle_status_report(&self, report: &str) {
        self.base.handshake_received.set();
        let report_in_inches = self.report_in_inches.load(Ordering::SeqCst);
        let mut state = {
            let current = self.base.state.lock().unwrap();
            parser::parse_state(report, &current, report_in_inches)
        };

        // If a job is active, 'Idle' between commands is reported as
        // 'Run'; while a hold was requested, the session owns the
        // pause state and forces 'Hold'.
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

    fn handle_error_line(&self, line: &str) {
        let code = line.split(':').nth(1).unwrap_or("").trim();
        self.base.set_device_error(parser::get_error(code));
        self.base.pending.finish_next(Some(line));

        if self.base.is_job_running() {
            log::error!("GRBL error during job: {line}. Halting stream.");
            *self.base.job_exception.lock().unwrap() =
                Some(SessionError::Device(format!("GRBL error: {line}")));
        }
    }

    fn handle_alarm(&self, code: &str, line: &str) {
        let error = super::errors::alarm_code_to_device_error(code);
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
            log::error!("GRBL ALARM during job: {line}. Halting stream.");
            *self.base.job_exception.lock().unwrap() =
                Some(SessionError::Device(format!("GRBL ALARM: {line}")));
        }
    }

    // ---------------------------------------------------------------
    // Command exchange
    // ---------------------------------------------------------------

    /// Send a realtime command (?, ~, !) directly: the firmware
    /// executes these on receipt and never acknowledges them.
    async fn send_realtime(&self, command: &str) -> Result<(), SessionError> {
        if !self.base.transport.is_open() {
            return Err(SessionError::Connection(
                "Serial transport disconnected".to_string(),
            ));
        }
        self.base.events.raw_tx(command);
        self.base.transport.write(command.as_bytes()).await?;
        Ok(())
    }

    // ---------------------------------------------------------------
    // Hold / device operations
    // ---------------------------------------------------------------

    pub async fn set_hold(&self, hold: bool) -> Result<(), SessionError> {
        self.base.is_holding.store(hold, Ordering::SeqCst);
        if !self.base.is_job_running() {
            self.base.is_cancelled.store(false, Ordering::SeqCst);
        }
        self.send_realtime(if hold { "!" } else { "~" }).await?;
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
        active_wcs: &str,
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

        // Some Grbl versions forget the G54 offset after homing;
        // toggling to another WCS and back re-activates it.
        let temp_wcs = if active_wcs == "G54" { "G55" } else { "G54" };
        self.execute_command("G4 P0.01".to_string()).await?;
        self.execute_command(temp_wcs.to_string()).await?;
        self.execute_command(active_wcs.to_string()).await?;
        self.base.clear_device_error();
        Ok(())
    }

    pub async fn read_settings(
        &self,
    ) -> Result<Vec<(String, String)>, SessionError> {
        let lines = PingPongSession::execute_interactive_command(
            self,
            "$$".to_string(),
        )
        .await?;
        self.report_in_inches
            .store(parser::is_report_in_inches(&lines), Ordering::SeqCst);
        Ok(parser::parse_setting_pairs(&lines))
    }

    pub async fn write_setting(
        &self,
        key: &str,
        value: &str,
    ) -> Result<(), SessionError> {
        self.execute_command(format!("${key}={value}")).await?;
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

    pub async fn read_wcs_offsets(
        &self,
    ) -> Result<HashMap<String, (f64, f64, f64)>, SessionError> {
        let lines = PingPongSession::execute_interactive_command(
            self,
            "$#".to_string(),
        )
        .await?;
        let report_in_inches = self.report_in_inches.load(Ordering::SeqCst);
        let mut offsets = HashMap::new();
        for line in &lines {
            if let Some((slot, (x, y, z))) = parser::parse_wcs_line(line) {
                let pos = if report_in_inches {
                    (
                        crate::core::types::inches_to_mm(x),
                        crate::core::types::inches_to_mm(y),
                        crate::core::types::inches_to_mm(z),
                    )
                } else {
                    (x, y, z)
                };
                offsets.insert(slot, pos);
            }
        }
        self.base.events.wcs_updated(&offsets);
        Ok(offsets)
    }

    pub async fn read_parser_state(
        &self,
    ) -> Result<Option<String>, SessionError> {
        let lines = PingPongSession::execute_interactive_command(
            self,
            "$G".to_string(),
        )
        .await?;
        Ok(parser::parse_grbl_parser_state(&lines))
    }

    pub async fn run_probe_cycle(
        &self,
        axis_letter: &str,
        max_travel: f64,
        feed_rate: f64,
    ) -> Result<Option<(f64, f64, f64)>, SessionError> {
        let axis_letter = axis_letter.to_uppercase();
        let dialect = self.base.dialect();
        let cmd = format_template(
            &dialect.probe_cycle,
            &[
                ("axis_letter", Arg::Str(axis_letter.clone())),
                ("max_travel", Arg::Length(max_travel)),
                ("feed_rate", Arg::Speed(feed_rate)),
            ],
        )
        .map_err(SessionError::Other)?;

        self.base
            .events
            .probe_status(&format!("Probing {axis_letter}..."));
        let lines =
            match PingPongSession::execute_interactive_command(self, cmd).await
            {
                Ok(lines) => lines,
                Err(_) => {
                    self.base.events.probe_status("Probe failed: Timed out");
                    return Ok(None);
                }
            };

        for line in &lines {
            if let Some(((x, y, z), success)) = parser::parse_probe_line(line) {
                if success {
                    let pos = if self.report_in_inches.load(Ordering::SeqCst) {
                        (
                            crate::core::types::inches_to_mm(x),
                            crate::core::types::inches_to_mm(y),
                            crate::core::types::inches_to_mm(z),
                        )
                    } else {
                        (x, y, z)
                    };
                    self.base.events.probe_status(&format!(
                        "Probe triggered at ({}, {}, {})",
                        pos.0, pos.1, pos.2
                    ));
                    return Ok(Some(pos));
                }
            }
        }
        self.base.events.probe_status("Probe failed");
        Ok(None)
    }

    pub async fn detect_unit_system(
        &self,
    ) -> Result<Option<UnitSystem>, SessionError> {
        let lines = match PingPongSession::execute_interactive_command(
            self,
            "$$".to_string(),
        )
        .await
        {
            Ok(lines) => lines,
            Err(err) => {
                log::warn!("Unit system detection failed: {err}");
                return Ok(None);
            }
        };
        self.report_in_inches
            .store(parser::is_report_in_inches(&lines), Ordering::SeqCst);
        Ok(parser::detect_unit_system_from_settings(&lines))
    }

    /// Abort a running job by sending a soft reset (Ctrl-X).
    pub async fn cancel(&self, emergency: bool) -> Result<(), SessionError> {
        cancel(
            self,
            emergency,
            StopCommand {
                bytes: b"\x18",
                log: "Sending Soft Reset (Ctrl-X) to device.",
            },
            "Serial transport not connected",
        )
        .await
    }
}

#[async_trait]
impl PingPongSession for SimpleCore {
    fn base(&self) -> &LineSessionBase {
        &self.base
    }

    fn handle_line(&self, line: &str) {
        if line == "ok" {
            self.base.events.raw_rx("ok");
            self.base.pending.finish_next(None);
            return;
        }
        if line.starts_with("error:") {
            self.base.events.raw_rx(line);
            self.handle_error_line(line);
            return;
        }
        if line.starts_with('<') && line.ends_with('>') {
            self.handle_status_report(line);
            return;
        }
        if let Some(code) = line.strip_prefix("ALARM:") {
            self.base.events.raw_rx(line);
            self.handle_alarm(code.trim(), line);
            return;
        }
        if line.starts_with("Grbl ") || line.starts_with("GrblHAL ") {
            self.base.events.raw_rx(line);
            self.base.handshake_received.set();
            return;
        }
        self.base.events.raw_rx(line);
        self.base.pending.collect(line);
    }

    async fn on_opened(&self) {
        self.base
            .connection_status(TransportStatus::Connected, None);
    }

    async fn on_established(&self) {
        // "Connected" was already reported when the transport
        // opened — before the handshake.
    }

    async fn handshake(&self) -> bool {
        self.await_handshake().await
    }

    fn on_handshake_failed(&self) {
        log::warn!("Handshake timeout. Retrying connection.");
    }

    async fn poll_loop(&self) {
        self.status_poll_loop().await;
    }

    async fn ping_pong(
        &self,
        command: &str,
    ) -> Result<Vec<String>, SessionError> {
        let _guard = self.base.cmd_lock.lock().await;
        if !self.base.transport.is_open() {
            return Err(SessionError::Connection(
                "Serial transport not connected".to_string(),
            ));
        }
        self.base
            .await_ack(command, secs(self.config.command_timeout))
            .await
    }

    fn job_log_label(&self) -> &'static str {
        "ping-pong"
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

    async fn run_raw(&self, machine_code: &str) {
        let lines: Vec<String> = machine_code
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        let (gcode, realtime) = parser::split_realtime_commands(&lines);
        for line in &realtime {
            if let Err(err) = self.send_realtime(line).await {
                log::error!("Realtime command failed: {err}");
                return;
            }
        }
        if gcode.is_empty() {
            return;
        }
        self.run(&gcode.join("\n"), HashMap::new()).await;
    }
}

/// The typed uniform session surface (see
/// [`core::session::Session`]): adapts the protocol-native methods,
/// which remain the behavior source.
#[async_trait]
impl Session for SimpleCore {
    async fn connect(self: Arc<Self>) -> Result<(), SessionError> {
        PingPongSession::connect(self).await;
        Ok(())
    }

    async fn cleanup(&self) {
        PingPongSession::cleanup(self).await;
    }

    async fn run(
        self: Arc<Self>,
        gcode: &str,
        op_map: &HashMap<usize, i64>,
        _op_estimates: &[f64],
    ) -> Result<(), SessionError> {
        PingPongSession::run(self.as_ref(), gcode, op_map.clone()).await;
        Ok(())
    }

    async fn cancel(&self, mode: CancelMode) -> Result<(), SessionError> {
        SimpleCore::cancel(self, mode == CancelMode::Immediate).await
    }

    async fn execute_interactive_command(
        &self,
        command: &str,
    ) -> Result<Vec<String>, SessionError> {
        PingPongSession::execute_interactive_command(self, command.to_string())
            .await
    }

    async fn home(
        &self,
        axes: Option<Vec<String>>,
        active_wcs: &str,
    ) -> Result<(), SessionError> {
        SimpleCore::home(self, axes, active_wcs).await
    }

    async fn move_to(
        &self,
        speed: f64,
        x: f64,
        y: f64,
        _z: Option<f64>,
    ) -> Result<(), SessionError> {
        PingPongSession::move_to(self, speed, x, y).await
    }

    async fn jog(
        &self,
        speed: f64,
        deltas: &[(String, f64)],
    ) -> Result<(), SessionError> {
        PingPongSession::jog(self, speed, deltas).await
    }

    async fn select_tool(&self, tool_number: i64) -> Result<(), SessionError> {
        PingPongSession::select_tool(self, tool_number).await
    }

    async fn set_power(&self, power: Option<f64>) -> Result<(), SessionError> {
        PingPongSession::set_power(self, power).await
    }

    async fn set_focus_power(
        &self,
        power: Option<f64>,
    ) -> Result<(), SessionError> {
        PingPongSession::set_focus_power(self, power).await
    }

    async fn set_hold(&self, hold: bool) -> Result<(), SessionError> {
        SimpleCore::set_hold(self, hold).await
    }

    async fn set_wcs_offset(
        &self,
        wcs_slot: &str,
        x: f64,
        y: f64,
        z: Option<f64>,
    ) -> Result<(), SessionError> {
        SimpleCore::set_wcs_offset(self, wcs_slot, x, y, z).await
    }

    async fn read_settings(
        &self,
    ) -> Result<Vec<(String, String)>, SessionError> {
        SimpleCore::read_settings(self).await
    }

    async fn write_setting(
        &self,
        key: &str,
        value: &str,
    ) -> Result<(), SessionError> {
        SimpleCore::write_setting(self, key, value).await
    }

    async fn read_wcs_offsets(
        &self,
    ) -> Result<HashMap<String, (f64, f64, f64)>, SessionError> {
        SimpleCore::read_wcs_offsets(self).await
    }

    async fn read_parser_state(&self) -> Result<Option<String>, SessionError> {
        SimpleCore::read_parser_state(self).await
    }

    async fn run_probe_cycle(
        &self,
        axis_letter: &str,
        max_travel: f64,
        feed_rate: f64,
    ) -> Result<Option<(f64, f64, f64)>, SessionError> {
        SimpleCore::run_probe_cycle(self, axis_letter, max_travel, feed_rate)
            .await
    }

    async fn detect_unit_system(
        &self,
    ) -> Result<Option<UnitSystem>, SessionError> {
        SimpleCore::detect_unit_system(self).await
    }
}
