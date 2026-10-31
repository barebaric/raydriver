//! The Marlin session: a minimal ping-pong session for Marlin
//! firmware over serial.
//!
//! Port of Rayforge's `MarlinSerialDriver` protocol behavior.  The
//! device announces itself with a `start` line (its absence marks a
//! phantom port), positions are polled with `M114` instead of `?`
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

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use std::collections::VecDeque;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::core::dialect::{format_template, Arg, Dialect};
use crate::core::session::{AsyncEvent, SessionError, SessionEvents};
use crate::core::transport::{Transport, TransportKind};
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

/// The Marlin session: connection lifecycle, M114 position polling
/// and ping-pong command exchange.
pub struct MarlinCore {
    config: MarlinConfig,
    dialect: RwLock<Dialect>,
    events: Arc<dyn SessionEvents>,
    transport: Arc<dyn Transport>,
    state: Mutex<DeviceState>,
    keep_running: AtomicBool,
    job_running: AtomicBool,
    is_cancelled: AtomicBool,
    handshake_received: AsyncEvent,
    connected: AsyncEvent,
    line_buffer: Mutex<Vec<u8>>,
    pending_queue: Mutex<VecDeque<Arc<PendingCmd>>>,
    job_exception: Mutex<Option<SessionError>>,
    last_reported_op_index: Mutex<i64>,
    cmd_lock: tokio::sync::Mutex<()>,
    connection_task: Mutex<Option<JoinHandle<()>>>,
    background_tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl MarlinCore {
    pub fn new(
        config: MarlinConfig,
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
            handshake_received: AsyncEvent::new(),
            connected: AsyncEvent::new(),
            line_buffer: Mutex::new(Vec::new()),
            pending_queue: Mutex::new(VecDeque::new()),
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
                    // The device needs a moment after `start`.
                    tokio::time::sleep(MarlinConfig::secs(
                        self.config.connect_settle,
                    ))
                    .await;
                    // Commands must not be sent before the device
                    // announced itself: the boot banner's trailing ok
                    // would otherwise resolve a pending command.
                    self.connected.set();
                    self.update_connection_status(
                        TransportStatus::Connected,
                        None,
                    );
                    self.position_poll_loop().await;
                } else {
                    log::warn!(
                        "No 'start' received. Port may be a phantom COM \
                         port without a connected device."
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
            tokio::time::sleep(MarlinConfig::secs(self.config.reconnect_delay))
                .await;
        }
    }

    /// Wait for the `start` handshake line; booting devices send it
    /// unprompted, so this simply waits.
    async fn await_handshake(&self) -> bool {
        let deadline = tokio::time::Instant::now()
            + MarlinConfig::secs(self.config.handshake_timeout);
        tokio::select! {
            _ = self.handshake_received.wait() => true,
            _ = tokio::time::sleep_until(deadline) => false,
        }
    }

    async fn position_poll_loop(&self) {
        loop {
            // Poll on an interval, counting from the connection: a
            // freshly booted device gets a moment before the first
            // M114.
            tokio::time::sleep(MarlinConfig::secs(
                self.config.status_poll_interval,
            ))
            .await;
            if !self.transport.is_open()
                || !self.keep_running.load(Ordering::SeqCst)
            {
                break;
            }
            if self.config.poll_status_while_running
                || !self.job_running.load(Ordering::SeqCst)
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
        if parser::is_ok(line) {
            self.events.raw_rx(line);
            self.finish_pending(Some(line));
            return;
        }
        if parser::is_error(line) {
            self.events.raw_rx(line);
            self.handle_error_line(line);
            return;
        }
        if line.trim() == "start" {
            self.events.raw_rx(line);
            self.handshake_received.set();
            self.collect_for_pending(line);
            return;
        }
        if parser::is_m114_position(line) {
            self.events.raw_rx(line);
            self.handle_position_report(line);
            self.collect_for_pending(line);
            return;
        }
        self.events.raw_rx(line);
        self.collect_for_pending(line);
    }

    fn collect_for_pending(&self, line: &str) {
        let front = self.pending_queue.lock().unwrap().front().cloned();
        if let Some(pending) = front {
            if !pending.finished.is_set() {
                pending.add_line(line);
            }
        }
    }

    /// Finish the front pending command (responses are wire-ordered,
    /// so the oldest outstanding command owns the next
    /// acknowledgement), optionally collecting one final response
    /// line first.  The waiter holds its own `Arc` and reads the
    /// lines from it.
    fn finish_pending(&self, last_line: Option<&str>) {
        let front = self.pending_queue.lock().unwrap().front().cloned();
        if let Some(pending) = front {
            if !pending.finished.is_set() {
                if let Some(line) = last_line {
                    pending.add_line(line);
                }
                pending.finish();
            }
        }
    }

    fn resolve_pending(&self) {
        let queued: Vec<_> =
            self.pending_queue.lock().unwrap().drain(..).collect();
        for pending in queued {
            pending.finish();
        }
    }

    fn handle_error_line(&self, line: &str) {
        let message = parser::parse_error_message(line);
        let snapshot = {
            let mut state = self.state.lock().unwrap();
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
            self.events.state_changed(&snapshot);
        }
        self.finish_pending(Some(line));

        if self.job_running.load(Ordering::SeqCst) {
            log::error!("Device error during job: {line}. Halting stream.");
            *self.job_exception.lock().unwrap() =
                Some(SessionError::Device(format!("Device error: {line}")));
        }
    }

    fn handle_position_report(&self, line: &str) {
        let Some((x, y, z)) = parser::parse_m114_line(line) else {
            return;
        };
        let snapshot = {
            let mut state = self.state.lock().unwrap();
            state.machine_pos = vec![Some(x), Some(y), Some(z)];
            state.work_pos = vec![Some(x), Some(y), Some(z)];
            if state.status == DeviceStatus::Unknown {
                state.status = DeviceStatus::Idle;
            }
            Some(state.clone())
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
        if !self.connected.is_set() {
            // Wait out the handshake: sending earlier would let the
            // boot banner's trailing ok resolve this command.
            let wait_deadline = tokio::time::Instant::now()
                + MarlinConfig::secs(
                    self.config.handshake_timeout
                        + self.config.connect_settle
                        + 2.0,
                );
            if tokio::time::timeout_at(wait_deadline, self.connected.wait())
                .await
                .is_err()
            {
                return Err(SessionError::Connection(
                    "device did not announce itself (no 'start')".to_string(),
                ));
            }
        }
        if !self.transport.is_open() {
            return Err(SessionError::Connection(
                "Serial transport not connected".to_string(),
            ));
        }
        self.enqueue_and_wait(command, true).await
    }

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
            Some(self.cmd_lock.lock().await)
        } else {
            None
        };
        if !self.transport.is_open() {
            return Err(SessionError::Connection(
                "Serial transport not connected".to_string(),
            ));
        }
        let pending = Arc::new(PendingCmd::new());
        self.pending_queue
            .lock()
            .unwrap()
            .push_back(pending.clone());
        self.events.raw_tx(command);
        let result = async {
            self.transport
                .write(format!("{command}\n").as_bytes())
                .await?;
            match tokio::time::timeout(
                MarlinConfig::secs(self.config.command_timeout),
                pending.finished.wait(),
            )
            .await
            {
                Ok(()) => Ok(()),
                Err(_) => Err(SessionError::Timeout),
            }
        }
        .await;
        // Drop the entry on every path: a late acknowledgement for a
        // timed out command must not resolve a later one.
        self.pending_queue
            .lock()
            .unwrap()
            .retain(|p| !Arc::ptr_eq(p, &pending));
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
    /// [`MarlinCore::execute_command`] — the protocol has no
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
        log::debug!("Starting Marlin job with {total} lines.");

        for (line_idx, raw_line) in lines.iter().enumerate() {
            if self.is_cancelled() {
                log::info!("Job cancelled. Stopping.");
                break;
            }

            let line = raw_line.trim();
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

        if had_exception || interrupted {
            self.send_safety_shutdown(true).await;
        }
        if !was_cancelled {
            self.events.job_finished();
        }
        log::debug!("Marlin streaming finished ({sent}/{total} lines).");
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
    // Cancel
    // ---------------------------------------------------------------

    pub async fn cancel(&self, emergency: bool) -> Result<(), SessionError> {
        log::debug!("Cancel command initiated.");
        let job_was_running = self.job_running.load(Ordering::SeqCst);
        self.is_cancelled.store(true, Ordering::SeqCst);
        self.job_running.store(false, Ordering::SeqCst);

        self.resolve_pending();
        self.line_buffer.lock().unwrap().clear();

        if !self.transport.is_open() {
            return Err(SessionError::Connection(
                "Serial transport not connected".to_string(),
            ));
        }
        log::info!("Sending M410 (Quick Stop) to device.");
        self.transport.write(b"M410\n").await?;

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
        let has_error = response.iter().any(|line| parser::is_error(line));
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
        let p_num = crate::grbl::parser::gcode_to_p_number(wcs_slot)
            .ok_or_else(|| {
                SessionError::Other(format!("Invalid WCS slot: {wcs_slot}"))
            })?;
        let dialect = self.dialect();
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
