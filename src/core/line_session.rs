//! Shared skeleton for the strict ping-pong line sessions (the
//! minimal GRBL session, Smoothie and Marlin): send one line, wait
//! for its `ok`/`error:` acknowledgement, then send the next.
//!
//! The protocol sessions keep their byte-for-byte behavior and their
//! public surface; this module owns the mechanics they used to
//! duplicate — the pending-acknowledgement bookkeeping, RX line
//! reassembly, the connection loop skeleton, job streaming, cancel
//! and the safety shutdown — so the quirks (late-ack suppression,
//! wire-ordered FIFO for Marlin, event-outside-the-lock emission)
//! exist exactly once.
//!
//! A session implements [`PingPongSession`]: a handful of protocol
//! hooks (line dispatch, handshake style, poll loop) plus the shared
//! state bundle ([`LineSessionBase`]) it returns from
//! [`PingPongSession::base`].

use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::core::dialect::{format_template, Arg, Dialect};
use crate::core::session::{AsyncEvent, SessionError, SessionEvents};
use crate::core::transport::Transport;
use crate::core::types::{
    DeviceError, DeviceState, DeviceStatus, TransportStatus,
};

/// One sent command awaiting its acknowledgement.
pub(crate) struct PendingCmd {
    lines: Mutex<Vec<String>>,
    finished: AsyncEvent,
}

impl PendingCmd {
    pub(crate) fn new() -> Self {
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

    fn is_finished(&self) -> bool {
        self.finished.is_set()
    }
}

/// Where outstanding pending acknowledgements live.
///
/// The slot model allows a single in-flight command (GRBL simple,
/// Smoothie); the FIFO queue matches the device's wire-ordered
/// responses when a status poll may be in flight while a user
/// command waits (Marlin).
pub(crate) enum PendingStore {
    Slot(Mutex<Option<Arc<PendingCmd>>>),
    Queue(Mutex<VecDeque<Arc<PendingCmd>>>),
}

impl PendingStore {
    pub(crate) fn slot() -> Self {
        Self::Slot(Mutex::new(None))
    }

    pub(crate) fn queue() -> Self {
        Self::Queue(Mutex::new(VecDeque::new()))
    }

    /// Register a newly sent command.
    fn enqueue(&self, pending: Arc<PendingCmd>) {
        match self {
            Self::Slot(slot) => *slot.lock().unwrap() = Some(pending),
            Self::Queue(queue) => queue.lock().unwrap().push_back(pending),
        }
    }

    /// Feed an unsolicited response line to the oldest outstanding
    /// command (unless it already finished).
    pub(crate) fn collect(&self, line: &str) {
        let pending = match self {
            Self::Slot(slot) => slot.lock().unwrap().clone(),
            Self::Queue(queue) => queue.lock().unwrap().front().cloned(),
        };
        if let Some(pending) = pending {
            if !pending.is_finished() {
                pending.add_line(line);
            }
        }
    }

    /// Finish the oldest outstanding command, optionally collecting
    /// one final response line first (the `error:`/`ALARM:` text).
    /// The entry stays owned by the waiting ping-pong.
    pub(crate) fn finish_next(&self, last_line: Option<&str>) {
        let pending = match self {
            Self::Slot(slot) => slot.lock().unwrap().clone(),
            Self::Queue(queue) => queue.lock().unwrap().front().cloned(),
        };
        if let Some(pending) = pending {
            if !pending.is_finished() {
                if let Some(line) = last_line {
                    pending.add_line(line);
                }
                pending.finish();
            }
        }
    }

    /// Drop one entry on every path: a late acknowledgement for a
    /// timed out command must not resolve a later one.  For the slot
    /// this is the same as [`PendingStore::resolve`].
    fn remove(&self, pending: &Arc<PendingCmd>) {
        match self {
            Self::Slot(slot) => {
                if let Some(pending) = slot.lock().unwrap().take() {
                    pending.finish();
                }
            }
            Self::Queue(queue) => {
                queue.lock().unwrap().retain(|p| !Arc::ptr_eq(p, pending));
            }
        }
    }

    /// Finish and drop every outstanding entry (cancel, cleanup).
    pub(crate) fn resolve(&self) {
        match self {
            Self::Slot(slot) => {
                if let Some(pending) = slot.lock().unwrap().take() {
                    pending.finish();
                }
            }
            Self::Queue(queue) => {
                for pending in queue.lock().unwrap().drain(..) {
                    pending.finish();
                }
            }
        }
    }
}

/// The state bundle shared by every ping-pong session.  Sessions
/// embed this and route their shared operations through it.
pub(crate) struct LineSessionBase {
    pub(crate) dialect: RwLock<Dialect>,
    pub(crate) events: Arc<dyn SessionEvents>,
    pub(crate) transport: Arc<dyn Transport>,
    pub(crate) state: Mutex<DeviceState>,
    pub(crate) keep_running: AtomicBool,
    pub(crate) job_running: AtomicBool,
    pub(crate) is_cancelled: AtomicBool,
    pub(crate) is_holding: AtomicBool,
    pub(crate) handshake_received: AsyncEvent,
    pub(crate) line_buffer: Mutex<Vec<u8>>,
    pub(crate) pending: PendingStore,
    pub(crate) job_exception: Mutex<Option<SessionError>>,
    pub(crate) last_reported_op_index: Mutex<i64>,
    pub(crate) cmd_lock: tokio::sync::Mutex<()>,
    connection_task: Mutex<Option<JoinHandle<()>>>,
    background_tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl LineSessionBase {
    pub(crate) fn new(
        dialect: Dialect,
        events: Arc<dyn SessionEvents>,
        transport: Arc<dyn Transport>,
        pending: PendingStore,
    ) -> Self {
        Self {
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
            pending,
            job_exception: Mutex::new(None),
            last_reported_op_index: Mutex::new(-1),
            cmd_lock: tokio::sync::Mutex::new(()),
            connection_task: Mutex::new(None),
            background_tasks: Mutex::new(Vec::new()),
        }
    }

    // -----------------------------------------------------------
    // Accessors
    // -----------------------------------------------------------

    pub(crate) fn state(&self) -> DeviceState {
        self.state.lock().unwrap().clone()
    }

    pub(crate) fn is_job_running(&self) -> bool {
        self.job_running.load(Ordering::SeqCst)
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.is_cancelled.load(Ordering::SeqCst)
    }

    pub(crate) fn resource_uri(&self) -> Option<String> {
        self.transport.resource_uri()
    }

    pub(crate) fn dialect(&self) -> Dialect {
        self.dialect.read().unwrap().clone()
    }

    pub(crate) fn update_dialect(&self, dialect: Dialect) {
        *self.dialect.write().unwrap() = dialect;
    }

    // -----------------------------------------------------------
    // Event emission
    // -----------------------------------------------------------

    pub(crate) fn connection_status(
        &self,
        status: TransportStatus,
        message: Option<&str>,
    ) {
        self.events.connection_status(status, message);
    }

    /// Store *state* and emit it, unless it is identical to the
    /// currently reported state.  The callback runs outside the lock
    /// (the callback attaches to Python).
    pub(crate) fn emit_state_if_changed(&self, state: DeviceState) {
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

    /// Record a device error if none is reported yet, emitting the
    /// state when it changed.
    pub(crate) fn set_device_error(&self, error: DeviceError) {
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

    /// Clear any reported device error and emit the state (used
    /// after successful homing or alarm clearing).
    pub(crate) fn clear_device_error(&self) {
        let state = {
            let mut state = self.state.lock().unwrap();
            state.error = None;
            state.clone()
        };
        self.events.state_changed(&state);
    }

    // -----------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------

    pub(crate) async fn cleanup(&self) {
        self.keep_running.store(false, Ordering::SeqCst);
        self.job_running.store(false, Ordering::SeqCst);
        self.pending.resolve();
        self.abort_background_tasks().await;
        self.transport.close().await;
        self.connection_status(TransportStatus::Disconnected, None);
    }

    pub(crate) async fn abort_background_tasks(&self) {
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

    /// Abort every background task without awaiting, for use from
    /// `Drop` where blocking is not allowed.  A session dropped
    /// without `disconnect()` must not leave its connection, polling
    /// and reader tasks running on the shared runtime.
    pub(crate) fn abort_background_tasks_now(&self) {
        self.keep_running.store(false, Ordering::SeqCst);
        if let Some(task) = self.connection_task.lock().unwrap().take() {
            task.abort();
        }
        for task in self.background_tasks.lock().unwrap().drain(..) {
            task.abort();
        }
    }

    pub(crate) async fn abort_readers(&self) {
        let tasks: Vec<_> =
            self.background_tasks.lock().unwrap().drain(..).collect();
        for task in tasks {
            task.abort();
            let _ = task.await;
        }
    }

    /// Spawn the connection loop for *session*.
    pub(crate) async fn connect<S: PingPongSession>(session: &Arc<S>) {
        session.base().cleanup().await;
        session.base().keep_running.store(true, Ordering::SeqCst);
        let core = session.clone();
        let task = tokio::spawn(async move { connection_loop(core).await });
        *session.base().connection_task.lock().unwrap() = Some(task);
    }

    // -----------------------------------------------------------
    // Command exchange
    // -----------------------------------------------------------

    /// The shared tail of every ping-pong: enqueue the pending
    /// acknowledgement, write the command, await its acknowledgement
    /// under *timeout*, then drop the entry.
    pub(crate) async fn await_ack(
        &self,
        command: &str,
        timeout: Duration,
    ) -> Result<Vec<String>, SessionError> {
        let pending = Arc::new(PendingCmd::new());
        self.pending.enqueue(pending.clone());
        self.events.raw_tx(command);
        let result = async {
            self.transport
                .write(format!("{command}\n").as_bytes())
                .await?;
            match tokio::time::timeout(timeout, pending.finished.wait()).await {
                Ok(()) => Ok(()),
                Err(_) => Err(SessionError::Timeout),
            }
        }
        .await;
        self.pending.remove(&pending);
        match result {
            Ok(()) => Ok(pending.lines()),
            Err(err) => Err(err),
        }
    }

    // -----------------------------------------------------------
    // Job streaming
    // -----------------------------------------------------------

    /// Shared job prologue: reset cancellation and exception state,
    /// mark the job running and report `Run`.
    pub(crate) fn start_job(&self) {
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

    /// Fire [`SessionEvents::command_done`] when the op index
    /// changes, before the op's first line is sent.
    fn fire_op_progress(&self, op_map: &HashMap<usize, i64>, line_idx: usize) {
        if let Some(op_index) = op_map.get(&line_idx).copied() {
            let mut last = self.last_reported_op_index.lock().unwrap();
            if op_index != *last {
                *last = op_index;
                drop(last);
                log::debug!("Firing command_done for op {op_index}");
                self.events.command_done(op_index);
            }
        }
    }
}

/// The connection loop skeleton: open the transport, spawn the RX
/// reader, run the protocol handshake, run the poll loop while the
/// connection holds, then tear down and sleep until reconnect.
async fn connection_loop<S: PingPongSession>(session: Arc<S>) {
    let base = session.base();
    while base.keep_running.load(Ordering::SeqCst) {
        base.handshake_received.clear();
        let (tx, mut rx) = mpsc::unbounded_channel();
        if let Err(err) = base.transport.open(tx).await {
            log::error!("Connection error: {err}");
            base.connection_status(
                TransportStatus::Error,
                Some(&err.to_string()),
            );
        } else {
            let reader_session = session.clone();
            let reader = tokio::spawn(async move {
                while let Some(data) = rx.recv().await {
                    reader_session.handle_rx(&data);
                }
            });
            base.background_tasks.lock().unwrap().push(reader);

            session.on_opened().await;
            if session.handshake().await {
                session.on_established().await;
                session.poll_loop().await;
            } else {
                session.on_handshake_failed();
            }

            base.abort_readers().await;
            base.transport.close().await;
            base.line_buffer.lock().unwrap().clear();
        }

        if !base.keep_running.load(Ordering::SeqCst) {
            break;
        }
        base.connection_status(TransportStatus::Sleeping, None);
        tokio::time::sleep(session.reconnect_delay()).await;
    }
}

/// The shared cancel sequence.  *stop* is the protocol's job-abort
/// bytes (sent raw, without a `raw_tx` event, exactly like the
/// sessions always did); *not_connected* is the error message used
/// when the transport is down.
pub(crate) async fn cancel<S: PingPongSession>(
    session: &S,
    emergency: bool,
    stop: StopCommand,
    not_connected: &str,
) -> Result<(), SessionError> {
    let base = session.base();
    log::debug!("Cancel command initiated.");
    let job_was_running = base.job_running.load(Ordering::SeqCst);
    base.is_cancelled.store(true, Ordering::SeqCst);
    base.job_running.store(false, Ordering::SeqCst);
    base.is_holding.store(false, Ordering::SeqCst);

    // Unblock a sender parked on a missing acknowledgement; the
    // stream loop observes the cancellation and stops.
    base.pending.resolve();
    base.line_buffer.lock().unwrap().clear();

    if !base.transport.is_open() {
        return Err(SessionError::Connection(not_connected.to_string()));
    }
    log::info!("{}", stop.log);
    base.transport.write(stop.bytes).await?;

    session.safety_shutdown(emergency).await;

    if job_was_running {
        base.events.job_finished();
    }
    Ok(())
}

/// The protocol-specific cancel payload.
pub(crate) struct StopCommand {
    pub(crate) bytes: &'static [u8],
    pub(crate) log: &'static str,
}

/// The ping-pong session contract: the shared mechanics live here as
/// provided methods; a protocol fills in the hooks.
#[async_trait]
pub(crate) trait PingPongSession: Send + Sync + 'static {
    /// The shared state bundle.
    fn base(&self) -> &LineSessionBase;

    // -------------------------------------------------------
    // Protocol hooks
    // -------------------------------------------------------

    /// Dispatch one complete RX line (status reports, acks, alarms,
    /// protocol-specific lines).
    fn handle_line(&self, line: &str);

    /// Wait for the protocol handshake after transport open.
    /// `false` marks a phantom port.
    async fn handshake(&self) -> bool;

    /// Called right after the transport opened, before the
    /// handshake.  Only sessions that report "Connected" before the
    /// handshake completes need this.
    async fn on_opened(&self) {}

    /// Called after a successful handshake, before the poll loop.
    /// The default reports "Connected".
    async fn on_established(&self) {
        self.base()
            .connection_status(TransportStatus::Connected, None);
    }

    /// Log and report a failed handshake (phantom port).
    fn on_handshake_failed(&self);

    /// The post-handshake poll loop (status reports or position
    /// polling); runs until the connection drops.
    async fn poll_loop(&self);

    /// Send one command and wait for its `ok`/`error:`
    /// acknowledgement.  Returns the collected response lines; an
    /// `error:` response is the last line.
    async fn ping_pong(
        &self,
        command: &str,
    ) -> Result<Vec<String>, SessionError>;

    /// Firmware label used in the job streaming log lines.
    fn job_log_label(&self) -> &'static str;

    /// Whether an `Alarm` device status stops the job stream.
    fn stop_job_on_alarm(&self) -> bool {
        true
    }

    /// Prepare one raw G-code line for sending.
    fn prep_job_line<'a>(&'a self, raw: &'a str) -> Cow<'a, str> {
        Cow::Borrowed(raw.trim())
    }

    /// Delay before the safety shutdown commands are sent; `None`
    /// sends them immediately.
    fn safety_shutdown_delay(&self) -> Option<Duration>;

    /// How long to sleep between reconnect attempts.
    fn reconnect_delay(&self) -> Duration;

    // -------------------------------------------------------
    // Provided mechanics
    // -------------------------------------------------------

    /// Reassemble received bytes into lines and dispatch each one.
    fn handle_rx(&self, data: &[u8]) {
        let base = self.base();
        let mut new_lines = Vec::new();
        {
            let mut buffer = base.line_buffer.lock().unwrap();
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

    /// Send a command and await its full response.
    async fn execute_command(
        &self,
        command: String,
    ) -> Result<Vec<String>, SessionError> {
        if !self.base().job_running.load(Ordering::SeqCst) {
            self.base().is_cancelled.store(false, Ordering::SeqCst);
        }
        self.ping_pong(&command).await
    }

    /// Send a command and await its full response, blocking any
    /// other command from interleaving (identical to
    /// [`PingPongSession::execute_command`] — the protocol has no
    /// separate queue).
    async fn execute_interactive_command(
        &self,
        command: String,
    ) -> Result<Vec<String>, SessionError> {
        self.execute_command(command).await
    }

    /// Stream a G-code job line by line, strict ping-pong.
    async fn run(&self, gcode: &str, op_map: HashMap<usize, i64>) {
        let base = self.base();
        base.start_job();
        let lines: Vec<String> = gcode.lines().map(|l| l.to_string()).collect();
        let total = lines.len();
        let mut sent = 0usize;
        let mut interrupted = false;
        log::debug!(
            "Starting {} job with {total} lines.",
            self.job_log_label()
        );

        for (line_idx, raw_line) in lines.iter().enumerate() {
            if base.is_cancelled()
                || (self.stop_job_on_alarm()
                    && base.state.lock().unwrap().status == DeviceStatus::Alarm)
            {
                if self.stop_job_on_alarm() {
                    log::info!("Job cancelled or alarm. Stopping.");
                } else {
                    log::info!("Job cancelled. Stopping.");
                }
                break;
            }

            let line = self.prep_job_line(raw_line);
            if line.is_empty() {
                continue;
            }

            base.fire_op_progress(&op_map, line_idx);

            match self.ping_pong(&line).await {
                Ok(_) => {
                    if base.job_exception.lock().unwrap().is_some() {
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

        let had_exception = base.job_exception.lock().unwrap().take().is_some();
        let was_cancelled = base.is_cancelled();
        base.job_running.store(false, Ordering::SeqCst);
        base.is_holding.store(false, Ordering::SeqCst);

        if had_exception || interrupted {
            self.safety_shutdown(true).await;
        }
        if !was_cancelled {
            base.events.job_finished();
        }
        log::debug!(
            "{} streaming finished ({sent}/{total} lines).",
            self.job_log_label()
        );
    }

    /// Stream pre-rendered machine code: trim, drop empties, run.
    async fn run_raw(&self, machine_code: &str) {
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

    /// Best-effort transmission of the dialect's tool-off commands
    /// so a cancelled or aborted job cannot leave persistent PWM
    /// outputs energized.  Fire-and-forget: responses are ignored.
    async fn safety_shutdown(&self, emergency: bool) {
        let base = self.base();
        let dialect = base.dialect();
        let mut commands = dialect.safety_off_commands.clone();
        if emergency {
            if let Some(cmd) = dialect.emergency_stop.clone() {
                commands.push(cmd);
            }
        }
        if commands.is_empty() || !base.transport.is_open() {
            return;
        }
        if let Some(delay) = self.safety_shutdown_delay() {
            tokio::time::sleep(delay).await;
        }
        for command in commands {
            if !base.transport.is_open() {
                break;
            }
            if let Err(err) = base
                .transport
                .write(format!("{command}\n").as_bytes())
                .await
            {
                log::warn!("Safety command '{command}' failed: {err}");
            }
        }
    }

    // -------------------------------------------------------
    // Dialect-formatted device operations
    // -------------------------------------------------------

    async fn move_to(
        &self,
        speed: f64,
        pos_x: f64,
        pos_y: f64,
    ) -> Result<(), SessionError> {
        let dialect = self.base().dialect();
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

    async fn jog(
        &self,
        speed: f64,
        deltas: &[(String, f64)],
    ) -> Result<(), SessionError> {
        let dialect = self.base().dialect();
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

    async fn select_tool(&self, tool_number: i64) -> Result<(), SessionError> {
        let dialect = self.base().dialect();
        let cmd = format_template(
            &dialect.tool_change,
            &[("tool_number", Arg::Int(tool_number))],
        )
        .map_err(SessionError::Other)?;
        self.execute_command(cmd).await?;
        Ok(())
    }

    async fn set_power(&self, power: Option<f64>) -> Result<(), SessionError> {
        let dialect = self.base().dialect();
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

    async fn set_focus_power(
        &self,
        power: Option<f64>,
    ) -> Result<(), SessionError> {
        let dialect = self.base().dialect();
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
}
