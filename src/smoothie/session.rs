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

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::core::dialect::{format_template, Arg, Dialect};
use crate::core::session::{AsyncEvent, SessionError, SessionEvents};
use crate::core::transport::{Transport, TransportKind};
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

    fn secs(v: f64) -> Duration {
        Duration::from_secs_f64(v.max(0.0))
    }
}

/// One sent command awaiting its acknowledgement.
struct PendingCmd {
    lines: Mutex<Vec<String>>,
    finished: AsyncEvent,
}

impl PendingCmd {
    fn new() -> Self {
        Self {
            lines: Mutex::new(Vec::new()),
            finished: AsyncEvent::new(),
        }
    }

    fn add_line(&self, line: &str) {
        self.lines.lock().unwrap().push(line.to_string());
    }

    fn finish(&self) {
        self.finished.set();
    }

    fn lines(&self) -> Vec<String> {
        self.lines.lock().unwrap().clone()
    }
}

/// The Smoothie session: connection lifecycle, status polling and
/// ping-pong command exchange.
pub struct SmoothieCore {
    config: SmoothieConfig,
    dialect: RwLock<Dialect>,
    events: Arc<dyn SessionEvents>,
    transport: Arc<dyn Transport>,
    state: Mutex<DeviceState>,
    keep_running: AtomicBool,
    job_running: AtomicBool,
    is_cancelled: AtomicBool,
    is_holding: AtomicBool,
    handshake_received: AsyncEvent,
    line_buffer: Mutex<Vec<u8>>,
    pending: Mutex<Option<Arc<PendingCmd>>>,
    job_exception: Mutex<Option<SessionError>>,
    last_reported_op_index: Mutex<i64>,
    cmd_lock: tokio::sync::Mutex<()>,
    connection_task: Mutex<Option<JoinHandle<()>>>,
    background_tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl SmoothieCore {
    pub fn new(
        config: SmoothieConfig,
        dialect: Dialect,
        events: Arc<dyn SessionEvents>,
    ) -> Arc<Self> {
        let transport = config.transport.build();
        Arc::new(Self {
            config,
            dialect: RwLock::new(dialect),
            events,
            transport,
            state: Mutex::new(DeviceState::default()),
            keep_running: AtomicBool::new(false),
            job_running: AtomicBool::new(false),
            is_cancelled: AtomicBool::new(false),
            is_holding: AtomicBool::new(false),
            handshake_received: AsyncEvent::new(),
            line_buffer: Mutex::new(Vec::new()),
            pending: Mutex::new(None),
            job_exception: Mutex::new(None),
            last_reported_op_index: Mutex::new(-1),
            cmd_lock: tokio::sync::Mutex::new(()),
            connection_task: Mutex::new(None),
            background_tasks: Mutex::new(Vec::new()),
        })
    }

    // ---------------------------------------------------------------
    // Accessors
    // ---------------------------------------------------------------

    pub fn state(&self) -> DeviceState {
        self.state.lock().unwrap().clone()
    }

    pub fn is_job_running(&self) -> bool {
        self.job_running.load(Ordering::SeqCst)
    }

    pub fn resource_uri(&self) -> Option<String> {
        self.transport.resource_uri()
    }

    pub fn update_dialect(&self, dialect: Dialect) {
        *self.dialect.write().unwrap() = dialect;
    }

    fn dialect(&self) -> Dialect {
        self.dialect.read().unwrap().clone()
    }

    fn is_cancelled(&self) -> bool {
        self.is_cancelled.load(Ordering::SeqCst)
    }

    fn is_holding(&self) -> bool {
        self.is_holding.load(Ordering::SeqCst)
    }

    // ---------------------------------------------------------------
    // Connect / disconnect
    // ---------------------------------------------------------------

    pub async fn connect(self: &Arc<Self>) {
        self.cleanup().await;
        self.keep_running.store(true, Ordering::SeqCst);
        let core = self.clone();
        let task = tokio::spawn(async move { core.connection_loop().await });
        *self.connection_task.lock().unwrap() = Some(task);
    }

    pub async fn cleanup(&self) {
        self.keep_running.store(false, Ordering::SeqCst);
        self.job_running.store(false, Ordering::SeqCst);
        self.resolve_pending();
        self.abort_background_tasks().await;
        self.transport.close().await;
        self.update_connection_status(TransportStatus::Disconnected, None);
    }

    async fn abort_background_tasks(&self) {
        let connection = self.connection_task.lock().unwrap().take();
        if let Some(task) = connection {
            task.abort();
            let _ = task.await;
        }
        let tasks: Vec<_> =
            self.background_tasks.lock().unwrap().drain(..).collect();
        for task in tasks {
            task.abort();
            let _ = task.await;
        }
    }

    async fn abort_readers(&self) {
        let tasks: Vec<_> =
            self.background_tasks.lock().unwrap().drain(..).collect();
        for task in tasks {
            task.abort();
            let _ = task.await;
        }
    }

    fn update_connection_status(
        &self,
        status: TransportStatus,
        message: Option<&str>,
    ) {
        self.events.connection_status(status, message);
    }

    async fn connection_loop(self: Arc<Self>) {
        while self.keep_running.load(Ordering::SeqCst) {
            self.handshake_received.clear();
            let (tx, mut rx) = mpsc::unbounded_channel();
            if let Err(err) = self.transport.open(tx).await {
                log::error!("Connection error: {err}");
                self.update_connection_status(
                    TransportStatus::Error,
                    Some(&err.to_string()),
                );
            } else {
                let core = self.clone();
                let reader = tokio::spawn(async move {
                    while let Some(data) = rx.recv().await {
                        core.handle_rx(&data);
                    }
                });
                self.background_tasks.lock().unwrap().push(reader);

                if self.await_handshake().await {
                    self.update_connection_status(
                        TransportStatus::Connected,
                        None,
                    );
                    self.status_poll_loop().await;
                } else {
                    log::warn!(
                        "No response from device. Port may be a phantom \
                         connection."
                    );
                    self.update_connection_status(
                        TransportStatus::Error,
                        Some("No response from device"),
                    );
                }

                self.abort_readers().await;
                self.transport.close().await;
                self.line_buffer.lock().unwrap().clear();
            }

            if !self.keep_running.load(Ordering::SeqCst) {
                break;
            }
            self.update_connection_status(TransportStatus::Sleeping, None);
            tokio::time::sleep(SmoothieConfig::secs(
                self.config.reconnect_delay,
            ))
            .await;
        }
    }

    /// Poll the device with `?` until a status report arrives —
    /// Smoothieware needs a moment after opening the link before it
    /// accepts commands, and silently drops earlier queries.
    async fn await_handshake(&self) -> bool {
        let deadline = tokio::time::Instant::now()
            + SmoothieConfig::secs(self.config.handshake_timeout);
        loop {
            if let Err(err) = self.transport.write(b"?").await {
                log::error!("Connection error during handshake: {err}");
                return false;
            }
            let poll_deadline = tokio::time::Instant::now()
                + SmoothieConfig::secs(self.config.handshake_poll_interval);
            tokio::select! {
                _ = self.handshake_received.wait() => return true,
                _ = tokio::time::sleep_until(poll_deadline) => {
                    if tokio::time::Instant::now() >= deadline {
                        return false;
                    }
                }
            }
        }
    }

    async fn status_poll_loop(&self) {
        while self.transport.is_open()
            && self.keep_running.load(Ordering::SeqCst)
        {
            if self.config.poll_status_while_running
                || !self.job_running.load(Ordering::SeqCst)
            {
                let _guard = self.cmd_lock.lock().await;
                if self.transport.is_open() {
                    if let Err(err) = self.transport.write(b"?").await {
                        log::warn!("Connection lost while polling: {err}");
                        break;
                    }
                }
            }
            tokio::time::sleep(SmoothieConfig::secs(
                self.config.status_poll_interval,
            ))
            .await;
        }
    }

    // ---------------------------------------------------------------
    // RX handling
    // ---------------------------------------------------------------

    fn handle_rx(&self, data: &[u8]) {
        let mut new_lines = Vec::new();
        {
            let mut buffer = self.line_buffer.lock().unwrap();
            buffer.extend_from_slice(data);
            while let Some(nl) = buffer.iter().position(|b| *b == b'\n') {
                let line_bytes: Vec<u8> = buffer.drain(..=nl).collect();
                if let Ok(text) = String::from_utf8(line_bytes) {
                    for line in text.trim().lines() {
                        if !line.is_empty() {
                            new_lines.push(line.to_string());
                        }
                    }
                }
            }
        }
        for line in new_lines {
            self.handle_line(&line);
        }
    }

    fn handle_line(&self, line: &str) {
        if line == "ok" {
            self.events.raw_rx("ok");
            self.finish_pending(Some("ok"));
            return;
        }
        if line.starts_with("error:") {
            self.events.raw_rx(line);
            self.handle_error_line(line);
            return;
        }
        if line.starts_with('<') && line.ends_with('>') {
            self.events.raw_rx(line);
            self.handle_status_report(line);
            return;
        }
        if let Some(code) = line.strip_prefix("ALARM:") {
            self.events.raw_rx(line);
            self.handle_alarm(code.trim(), line);
            return;
        }
        self.events.raw_rx(line);
        if let Some(pending) = self.pending_slot() {
            if !pending.finished.is_set() {
                pending.add_line(line);
            }
        }
    }

    fn pending_slot(&self) -> Option<Arc<PendingCmd>> {
        self.pending.lock().unwrap().clone()
    }

    /// Finish the pending command, optionally collecting one final
    /// response line first.  The slot stays owned by the waiting
    /// ping-pong until it takes over.
    fn finish_pending(&self, last_line: Option<&str>) {
        if let Some(pending) = self.pending_slot() {
            if !pending.finished.is_set() {
                if let Some(line) = last_line {
                    pending.add_line(line);
                }
                pending.finish();
            }
        }
    }

    fn resolve_pending(&self) {
        let pending = self.pending.lock().unwrap().take();
        if let Some(pending) = pending {
            pending.finish();
        }
    }

    fn handle_error_line(&self, line: &str) {
        let code = line.split(':').nth(1).unwrap_or("").trim();
        let error = parser::get_error(code);
        let snapshot = {
            let mut state = self.state.lock().unwrap();
            if state.error.is_none() {
                state.error = Some(error);
                Some(state.clone())
            } else {
                None
            }
        };
        if let Some(snapshot) = snapshot {
            self.events.state_changed(&snapshot);
        }
        self.finish_pending(Some(line));

        if self.job_running.load(Ordering::SeqCst) {
            log::error!("Device error during job: {line}. Halting stream.");
            *self.job_exception.lock().unwrap() =
                Some(SessionError::Device(format!("Device error: {line}")));
        }
    }

    fn handle_alarm(&self, code: &str, line: &str) {
        let error = crate::grbl::errors::alarm_code_to_device_error(code);
        let snapshot = {
            let mut state = self.state.lock().unwrap();
            if state.error.is_none() {
                state.error = Some(error);
            }
            state.status = DeviceStatus::Alarm;
            Some(state.clone())
        };
        if let Some(snapshot) = snapshot {
            self.events.state_changed(&snapshot);
        }
        self.finish_pending(Some(line));

        if self.job_running.load(Ordering::SeqCst) {
            log::error!("Device ALARM during job: {line}. Halting stream.");
            *self.job_exception.lock().unwrap() =
                Some(SessionError::Device(format!("Device ALARM: {line}")));
        }
    }

    /// Parse a Grbl-style status report (Smoothie emits them for
    /// `?`; see the module docs for why the GRBL parser is reused).
    fn handle_status_report(&self, report: &str) {
        self.handshake_received.set();
        let mut state = {
            let current = self.state.lock().unwrap();
            parser::parse_state(report, &current, false)
        };

        // While a job runs, 'Idle' between commands is reported as
        // 'Run'; a requested hold owns the pause state.
        if self.job_running.load(Ordering::SeqCst)
            && state.status == DeviceStatus::Idle
        {
            state.status = DeviceStatus::Run;
        }
        if self.is_holding() && state.status != DeviceStatus::Alarm {
            state.status = DeviceStatus::Hold;
        }

        let snapshot = {
            let mut current = self.state.lock().unwrap();
            if *current == state {
                None
            } else {
                *current = state.clone();
                Some(state)
            }
        };
        if let Some(snapshot) = snapshot {
            self.events.state_changed(&snapshot);
        }
    }

    // ---------------------------------------------------------------
    // Command exchange
    // ---------------------------------------------------------------

    /// Send one command and wait for its `ok`/`error:`
    /// acknowledgement.  Returns the collected response lines; an
    /// `error:` response is the last line.
    async fn ping_pong(
        &self,
        command: &str,
    ) -> Result<Vec<String>, SessionError> {
        let _guard = self.cmd_lock.lock().await;
        if !self.transport.is_open() {
            return Err(SessionError::Connection(
                "Transport not connected".to_string(),
            ));
        }
        let pending = Arc::new(PendingCmd::new());
        *self.pending.lock().unwrap() = Some(pending.clone());
        if !self.job_running.load(Ordering::SeqCst) {
            self.events.raw_tx(command);
        }
        let result = async {
            self.transport
                .write(format!("{command}\n").as_bytes())
                .await?;
            match tokio::time::timeout(
                SmoothieConfig::secs(self.config.command_timeout),
                pending.finished.wait(),
            )
            .await
            {
                Ok(()) => Ok(()),
                Err(_) => Err(SessionError::Timeout),
            }
        }
        .await;
        // Always drop the slot: a late acknowledgement for a timed
        // out command must not resolve a later one.
        self.resolve_pending();
        match result {
            Ok(()) => Ok(pending.lines()),
            Err(err) => Err(err),
        }
    }

    /// Send a command and await its full response.
    pub async fn execute_command(
        &self,
        command: String,
    ) -> Result<Vec<String>, SessionError> {
        if !self.job_running.load(Ordering::SeqCst) {
            self.is_cancelled.store(false, Ordering::SeqCst);
        }
        self.ping_pong(&command).await
    }

    /// Send a command and await its full response, blocking any
    /// other command from interleaving (identical to
    /// [`SmoothieCore::execute_command`] — the protocol has no
    /// separate queue).
    pub async fn execute_interactive_command(
        &self,
        command: String,
    ) -> Result<Vec<String>, SessionError> {
        self.execute_command(command).await
    }

    // ---------------------------------------------------------------
    // Job streaming
    // ---------------------------------------------------------------

    fn start_job(&self) {
        self.is_cancelled.store(false, Ordering::SeqCst);
        self.job_running.store(true, Ordering::SeqCst);
        self.is_holding.store(false, Ordering::SeqCst);
        *self.last_reported_op_index.lock().unwrap() = -1;
        *self.job_exception.lock().unwrap() = None;
        let state = {
            let mut state = self.state.lock().unwrap();
            state.status = DeviceStatus::Run;
            state.clone()
        };
        self.events.state_changed(&state);
    }

    pub async fn run(&self, gcode: &str, op_map: HashMap<usize, i64>) {
        self.start_job();
        let lines: Vec<String> = gcode.lines().map(|l| l.to_string()).collect();
        let total = lines.len();
        let mut sent = 0usize;
        let mut interrupted = false;
        log::debug!("Starting Smoothie job with {total} lines.");

        for (line_idx, raw_line) in lines.iter().enumerate() {
            if self.is_cancelled()
                || self.state.lock().unwrap().status == DeviceStatus::Alarm
            {
                log::info!("Job cancelled or alarm. Stopping.");
                break;
            }

            let line = parser::strip_gcode_comments(raw_line);
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Op progress fires when the op index changes, before
            // the op's first line is sent.
            if let Some(op_index) = op_map.get(&line_idx).copied() {
                let mut last = self.last_reported_op_index.lock().unwrap();
                if op_index != *last {
                    *last = op_index;
                    drop(last);
                    log::debug!("Firing command_done for op {op_index}");
                    self.events.command_done(op_index);
                }
            }

            match self.ping_pong(line).await {
                Ok(_) => {
                    if self.job_exception.lock().unwrap().is_some() {
                        break;
                    }
                    sent += 1;
                }
                Err(err) => {
                    log::warn!("Job interrupted: {err:?}");
                    interrupted = true;
                    break;
                }
            }
            tokio::task::yield_now().await;
        }

        let had_exception = self.job_exception.lock().unwrap().take().is_some();
        let was_cancelled = self.is_cancelled();
        self.job_running.store(false, Ordering::SeqCst);
        self.is_holding.store(false, Ordering::SeqCst);

        if had_exception || interrupted {
            self.send_safety_shutdown(true).await;
        }
        if !was_cancelled {
            self.events.job_finished();
        }
        log::debug!("Smoothie streaming finished ({sent}/{total} lines).");
    }

    pub async fn run_raw(&self, machine_code: &str) {
        let lines: Vec<String> = machine_code
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if lines.is_empty() {
            return;
        }
        self.run(&lines.join("\n"), HashMap::new()).await;
    }

    // ---------------------------------------------------------------
    // Cancel / hold
    // ---------------------------------------------------------------

    pub async fn cancel(&self, emergency: bool) -> Result<(), SessionError> {
        log::debug!("Cancel command initiated.");
        let job_was_running = self.job_running.load(Ordering::SeqCst);
        self.is_cancelled.store(true, Ordering::SeqCst);
        self.job_running.store(false, Ordering::SeqCst);
        self.is_holding.store(false, Ordering::SeqCst);

        self.resolve_pending();
        self.line_buffer.lock().unwrap().clear();

        if !self.transport.is_open() {
            return Err(SessionError::Connection(
                "Transport not connected".to_string(),
            ));
        }
        // Smoothieware aborts the job on Ctrl+C.
        log::info!("Sending Ctrl+C to device.");
        self.transport.write(b"\x03").await?;

        self.send_safety_shutdown(emergency).await;

        if job_was_running {
            self.events.job_finished();
        }
        Ok(())
    }

    /// Best-effort transmission of the dialect's tool-off commands
    /// so a cancelled or aborted job cannot leave persistent PWM
    /// outputs energized.  Fire-and-forget: responses are ignored.
    async fn send_safety_shutdown(&self, emergency: bool) {
        let dialect = self.dialect();
        let mut commands = dialect.safety_off_commands.clone();
        if emergency {
            if let Some(cmd) = dialect.emergency_stop.clone() {
                commands.push(cmd);
            }
        }
        if commands.is_empty() || !self.transport.is_open() {
            return;
        }
        tokio::time::sleep(SmoothieConfig::secs(
            self.config.safety_shutdown_delay,
        ))
        .await;
        for command in commands {
            if !self.transport.is_open() {
                break;
            }
            if let Err(err) = self
                .transport
                .write(format!("{command}\n").as_bytes())
                .await
            {
                log::warn!("Safety command '{command}' failed: {err}");
            }
        }
    }

    pub async fn set_hold(&self, hold: bool) -> Result<(), SessionError> {
        self.is_holding.store(hold, Ordering::SeqCst);
        if !self.transport.is_open() {
            return Err(SessionError::Connection(
                "Transport not connected".to_string(),
            ));
        }
        self.transport.write(if hold { b"!" } else { b"~" }).await?;
        let desired = if hold {
            DeviceStatus::Hold
        } else if self.job_running.load(Ordering::SeqCst) {
            DeviceStatus::Run
        } else {
            DeviceStatus::Idle
        };
        let snapshot = {
            let mut state = self.state.lock().unwrap();
            if state.status != desired {
                state.status = desired;
                Some(state.clone())
            } else {
                None
            }
        };
        if let Some(snapshot) = snapshot {
            self.events.state_changed(&snapshot);
        }
        Ok(())
    }

    // ---------------------------------------------------------------
    // Device operations
    // ---------------------------------------------------------------

    pub async fn home(
        &self,
        axes: Option<Vec<String>>,
    ) -> Result<(), SessionError> {
        let dialect = self.dialect();
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
        let snapshot = {
            let mut state = self.state.lock().unwrap();
            state.error = None;
            state.clone()
        };
        self.events.state_changed(&snapshot);
        Ok(())
    }

    pub async fn move_to(
        &self,
        speed: f64,
        pos_x: f64,
        pos_y: f64,
    ) -> Result<(), SessionError> {
        let dialect = self.dialect();
        let cmd = format_template(
            &dialect.move_to,
            &[
                ("speed", Arg::Speed(speed)),
                ("x", Arg::Length(pos_x)),
                ("y", Arg::Length(pos_y)),
            ],
        )
        .map_err(SessionError::Other)?;
        self.execute_command(cmd).await?;
        Ok(())
    }

    pub async fn jog(
        &self,
        speed: f64,
        deltas: &[(String, f64)],
    ) -> Result<(), SessionError> {
        let dialect = self.dialect();
        let head =
            format_template(&dialect.jog, &[("speed", Arg::Speed(speed))])
                .map_err(SessionError::Other)?;
        let mut cmd_parts = vec![head];
        for (axis_name, distance) in deltas {
            cmd_parts.push(format!(
                "{}{}",
                axis_name.to_uppercase(),
                crate::core::dialect::python_float_repr(*distance)
            ));
        }
        if cmd_parts.len() == 1 {
            return Ok(());
        }
        let cmd = cmd_parts.join(" ");
        self.execute_command(cmd).await?;
        Ok(())
    }

    pub async fn select_tool(
        &self,
        tool_number: i64,
    ) -> Result<(), SessionError> {
        let dialect = self.dialect();
        let cmd = format_template(
            &dialect.tool_change,
            &[("tool_number", Arg::Int(tool_number))],
        )
        .map_err(SessionError::Other)?;
        self.execute_command(cmd).await?;
        Ok(())
    }

    pub async fn clear_alarm(&self) -> Result<(), SessionError> {
        let dialect = self.dialect();
        let response =
            self.execute_command(dialect.clear_alarm.clone()).await?;
        let has_error = response.iter().any(|line| line.starts_with("error:"));
        if !has_error {
            let snapshot = {
                let mut state = self.state.lock().unwrap();
                state.error = None;
                state.clone()
            };
            self.events.state_changed(&snapshot);
        }
        Ok(())
    }

    pub async fn set_power(
        &self,
        power: Option<f64>,
    ) -> Result<(), SessionError> {
        let dialect = self.dialect();
        let cmd = match power {
            Some(p) if p > 0.0 => {
                format_template(&dialect.laser_on, &[("power", Arg::Length(p))])
                    .map_err(SessionError::Other)?
            }
            _ => dialect.laser_off.clone(),
        };
        self.execute_command(cmd).await?;
        Ok(())
    }

    pub async fn set_focus_power(
        &self,
        power: Option<f64>,
    ) -> Result<(), SessionError> {
        let dialect = self.dialect();
        let cmd = match power {
            Some(p) if p > 0.0 => format_template(
                &dialect.focus_laser_on,
                &[("power", Arg::Length(p))],
            )
            .map_err(SessionError::Other)?,
            _ => dialect.laser_off.clone(),
        };
        self.execute_command(cmd).await?;
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
        let dialect = self.dialect();
        let cmd = dialect.format_wcs_offset(p_num, x, y, z);
        self.execute_command(cmd).await?;
        Ok(())
    }
}
