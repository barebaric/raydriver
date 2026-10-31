//! A deliberately minimal GRBL session: ping-pong only.
//!
//! Port of Rayforge's `GrblSerialSimpleDriver`: send one line, wait
//! for its `ok`/`error:` acknowledgement, then send the next.  There
//! is no character-counting flow control, no stall detection, no
//! deadlock recovery and no buffer management — the simplicity is
//! the point: this session is for hardware where the full driver
//! misbehaves, and it must not inherit the full driver's complexity.
//!
//! It shares only the leaf building blocks with the full session:
//! transports, the parser, the dialect templates, the value types
//! and the [`SessionEvents`] notification trait.  Behavioural
//! contract difference: interactive commands return an `error:` line
//! as their last response line instead of raising (the raydriver
//! contract), while an error response during a job halts the stream
//! and triggers the safety shutdown.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::dialect::{format_template, Arg, GrblDialect};
use super::parser;
use super::session::{AsyncEvent, SessionError, SessionEvents};
use super::transport::{Transport, TransportKind};
use super::types::{
    DeviceError, DeviceState, DeviceStatus, TransportStatus, UnitSystem,
};

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

/// The simple session: connection lifecycle, status polling and
/// ping-pong command exchange, all in one small state machine.
pub struct SimpleCore {
    config: SimpleConfig,
    dialect: RwLock<GrblDialect>,
    events: Arc<dyn SessionEvents>,
    transport: Arc<dyn Transport>,
    state: Mutex<DeviceState>,
    keep_running: AtomicBool,
    job_running: AtomicBool,
    is_cancelled: AtomicBool,
    is_holding: AtomicBool,
    report_in_inches: AtomicBool,
    handshake_received: AsyncEvent,
    line_buffer: Mutex<Vec<u8>>,
    pending: Mutex<Option<Arc<PendingCmd>>>,
    job_exception: Mutex<Option<SessionError>>,
    last_reported_op_index: Mutex<i64>,
    cmd_lock: tokio::sync::Mutex<()>,
    connection_task: Mutex<Option<JoinHandle<()>>>,
    background_tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl SimpleCore {
    pub fn new(
        config: SimpleConfig,
        dialect: GrblDialect,
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
            report_in_inches: AtomicBool::new(false),
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

    pub fn update_dialect(&self, dialect: GrblDialect) {
        *self.dialect.write().unwrap() = dialect;
    }

    fn dialect(&self) -> GrblDialect {
        self.dialect.read().unwrap().clone()
    }

    fn is_cancelled(&self) -> bool {
        self.is_cancelled.load(Ordering::SeqCst)
    }

    fn is_holding(&self) -> bool {
        self.is_holding.load(Ordering::SeqCst)
    }

    fn emit_state_if_changed(&self, state: DeviceState) {
        let snapshot = {
            let mut current = self.state.lock().unwrap();
            if *current == state {
                None
            } else {
                *current = state.clone();
                Some(state)
            }
        };
        // Emit outside the lock (the callback attaches to Python).
        if let Some(snapshot) = snapshot {
            self.events.state_changed(&snapshot);
        }
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

                self.update_connection_status(TransportStatus::Connected, None);
                if self.await_handshake().await {
                    self.status_poll_loop().await;
                } else {
                    log::warn!("Handshake timeout. Retrying connection.");
                }

                self.abort_background_tasks().await;
                self.transport.close().await;
                self.line_buffer.lock().unwrap().clear();
            }

            if !self.keep_running.load(Ordering::SeqCst) {
                break;
            }
            self.update_connection_status(TransportStatus::Sleeping, None);
            tokio::time::sleep(SimpleConfig::secs(self.config.reconnect_delay))
                .await;
        }
    }

    /// Poll the device with `?` until it answers, bounded by the
    /// handshake timeout.  Devices that are still booting silently
    /// drop queries sent before their serial stream is ready.
    async fn await_handshake(&self) -> bool {
        let deadline = tokio::time::Instant::now()
            + SimpleConfig::secs(self.config.handshake_timeout);
        loop {
            if let Err(err) = self.transport.write(b"?").await {
                log::error!("Connection error during handshake: {err}");
                return false;
            }
            let poll_deadline = tokio::time::Instant::now()
                + SimpleConfig::secs(self.config.handshake_poll_interval);
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
            tokio::time::sleep(SimpleConfig::secs(
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
            self.finish_pending(None);
            return;
        }
        if line.starts_with("error:") {
            self.events.raw_rx(line);
            self.handle_error_line(line);
            return;
        }
        if line.starts_with('<') && line.ends_with('>') {
            self.handle_status_report(line);
            return;
        }
        if let Some(code) = line.strip_prefix("ALARM:") {
            self.events.raw_rx(line);
            self.handle_alarm(code.trim(), line);
            return;
        }
        if line.starts_with("Grbl ") || line.starts_with("GrblHAL ") {
            self.events.raw_rx(line);
            self.handshake_received.set();
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
    /// response line first (the `error:`/`ALARM:` text).  The slot
    /// itself stays owned by the waiting [`SimpleCore::ping_pong`].
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

    /// Finish and drop the pending command without collecting
    /// anything (cancel, cleanup).
    fn resolve_pending(&self) {
        let pending = self.pending.lock().unwrap().take();
        if let Some(pending) = pending {
            pending.finish();
        }
    }

    fn set_device_error(&self, error: DeviceError) {
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
    }

    fn handle_error_line(&self, line: &str) {
        let code = line.split(':').nth(1).unwrap_or("").trim();
        self.set_device_error(parser::get_error(code));
        self.finish_pending(Some(line));

        if self.job_running.load(Ordering::SeqCst) {
            log::error!("GRBL error during job: {line}. Halting stream.");
            *self.job_exception.lock().unwrap() =
                Some(SessionError::Device(format!("GRBL error: {line}")));
        }
    }

    fn handle_alarm(&self, code: &str, line: &str) {
        let error = super::errors::alarm_code_to_device_error(code);
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
            log::error!("GRBL ALARM during job: {line}. Halting stream.");
            *self.job_exception.lock().unwrap() =
                Some(SessionError::Device(format!("GRBL ALARM: {line}")));
        }
    }

    fn handle_status_report(&self, report: &str) {
        self.handshake_received.set();
        let report_in_inches = self.report_in_inches.load(Ordering::SeqCst);
        let mut state = {
            let current = self.state.lock().unwrap();
            parser::parse_state(report, &current, report_in_inches)
        };

        // If a job is active, 'Idle' between commands is reported as
        // 'Run'; while a hold was requested, the session owns the
        // pause state and forces 'Hold'.
        if self.job_running.load(Ordering::SeqCst)
            && state.status == DeviceStatus::Idle
        {
            state.status = DeviceStatus::Run;
        }
        if self.is_holding() && state.status != DeviceStatus::Alarm {
            state.status = DeviceStatus::Hold;
        }

        self.emit_state_if_changed(state);
    }

    // ---------------------------------------------------------------
    // Command exchange
    // ---------------------------------------------------------------

    /// Send one command and wait for its `ok`/`error:`
    /// acknowledgement (the ping-pong).  Returns the collected
    /// response lines; an `error:` response is the last line.
    async fn ping_pong(
        &self,
        command: &str,
    ) -> Result<Vec<String>, SessionError> {
        let _guard = self.cmd_lock.lock().await;
        if !self.transport.is_open() {
            return Err(SessionError::Connection(
                "Serial transport not connected".to_string(),
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
                SimpleConfig::secs(self.config.command_timeout),
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
    /// other command from interleaving.  Same as
    /// [`SimpleCore::execute_command`] — the simple protocol has no
    /// separate interactive path.
    pub async fn execute_interactive_command(
        &self,
        command: String,
    ) -> Result<Vec<String>, SessionError> {
        self.execute_command(command).await
    }

    /// Send a realtime command (?, ~, !) directly: the firmware
    /// executes these on receipt and never acknowledges them.
    async fn send_realtime(&self, command: &str) -> Result<(), SessionError> {
        if !self.transport.is_open() {
            return Err(SessionError::Connection(
                "Serial transport not connected".to_string(),
            ));
        }
        self.transport.write(command.as_bytes()).await?;
        Ok(())
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
        log::debug!("Starting ping-pong job with {total} lines.");

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
        log::debug!("Ping-pong streaming finished ({sent}/{total} lines).");
    }

    pub async fn run_raw(&self, machine_code: &str) {
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

    // ---------------------------------------------------------------
    // Cancel / hold
    // ---------------------------------------------------------------

    pub async fn cancel(&self, emergency: bool) -> Result<(), SessionError> {
        log::debug!("Cancel command initiated.");
        let job_was_running = self.job_running.load(Ordering::SeqCst);
        self.is_cancelled.store(true, Ordering::SeqCst);
        self.job_running.store(false, Ordering::SeqCst);
        self.is_holding.store(false, Ordering::SeqCst);

        // Unblock a sender parked on a missing acknowledgement; the
        // stream loop observes the cancellation and stops.
        self.resolve_pending();
        self.line_buffer.lock().unwrap().clear();

        if !self.transport.is_open() {
            return Err(SessionError::Connection(
                "Serial transport not connected".to_string(),
            ));
        }
        log::info!("Sending Soft Reset (Ctrl-X) to device.");
        self.transport.write(b"\x18").await?;

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
        tokio::time::sleep(SimpleConfig::secs(
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
        if !self.job_running.load(Ordering::SeqCst) {
            self.is_cancelled.store(false, Ordering::SeqCst);
        }
        self.send_realtime(if hold { "!" } else { "~" }).await?;
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
        active_wcs: &str,
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

        // Some Grbl versions forget the G54 offset after homing;
        // toggling to another WCS and back re-activates it.
        let temp_wcs = if active_wcs == "G54" { "G55" } else { "G54" };
        self.execute_command("G4 P0.01".to_string()).await?;
        self.execute_command(temp_wcs.to_string()).await?;
        self.execute_command(active_wcs.to_string()).await?;
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
                super::dialect::python_float_repr(*distance)
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

    pub async fn read_settings(
        &self,
    ) -> Result<Vec<(String, String)>, SessionError> {
        let lines = self.execute_interactive_command("$$".to_string()).await?;
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
        let dialect = self.dialect();
        let cmd = dialect.format_wcs_offset(p_num, x, y, z);
        self.execute_command(cmd).await?;
        Ok(())
    }

    pub async fn read_wcs_offsets(
        &self,
    ) -> Result<HashMap<String, (f64, f64, f64)>, SessionError> {
        let lines = self.execute_interactive_command("$#".to_string()).await?;
        let report_in_inches = self.report_in_inches.load(Ordering::SeqCst);
        let mut offsets = HashMap::new();
        for line in &lines {
            if let Some((slot, (x, y, z))) = parser::parse_wcs_line(line) {
                let pos = if report_in_inches {
                    (
                        super::types::inches_to_mm(x),
                        super::types::inches_to_mm(y),
                        super::types::inches_to_mm(z),
                    )
                } else {
                    (x, y, z)
                };
                offsets.insert(slot, pos);
            }
        }
        self.events.wcs_updated(&offsets);
        Ok(offsets)
    }

    pub async fn read_parser_state(
        &self,
    ) -> Result<Option<String>, SessionError> {
        let lines = self.execute_interactive_command("$G".to_string()).await?;
        Ok(parser::parse_grbl_parser_state(&lines))
    }

    pub async fn run_probe_cycle(
        &self,
        axis_letter: &str,
        max_travel: f64,
        feed_rate: f64,
    ) -> Result<Option<(f64, f64, f64)>, SessionError> {
        let axis_letter = axis_letter.to_uppercase();
        let dialect = self.dialect();
        let cmd = format_template(
            &dialect.probe_cycle,
            &[
                ("axis_letter", Arg::Str(axis_letter.clone())),
                ("max_travel", Arg::Length(max_travel)),
                ("feed_rate", Arg::Speed(feed_rate)),
            ],
        )
        .map_err(SessionError::Other)?;

        self.events
            .probe_status(&format!("Probing {axis_letter}..."));
        let lines = match self.execute_interactive_command(cmd).await {
            Ok(lines) => lines,
            Err(_) => {
                self.events.probe_status("Probe failed: Timed out");
                return Ok(None);
            }
        };

        for line in &lines {
            if let Some(((x, y, z), success)) = parser::parse_probe_line(line) {
                if success {
                    let pos = if self.report_in_inches.load(Ordering::SeqCst) {
                        (
                            super::types::inches_to_mm(x),
                            super::types::inches_to_mm(y),
                            super::types::inches_to_mm(z),
                        )
                    } else {
                        (x, y, z)
                    };
                    self.events.probe_status(&format!(
                        "Probe triggered at ({}, {}, {})",
                        pos.0, pos.1, pos.2
                    ));
                    return Ok(Some(pos));
                }
            }
        }
        self.events.probe_status("Probe failed");
        Ok(None)
    }

    pub async fn detect_unit_system(
        &self,
    ) -> Result<Option<UnitSystem>, SessionError> {
        let lines =
            match self.execute_interactive_command("$$".to_string()).await {
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
}
