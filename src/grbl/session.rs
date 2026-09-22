//! The GRBL session: connection lifecycle, handshake, status
//! polling, interactive command queue, and job streaming with stall
//! detection and deadlock recovery.
//!
//! This is a faithful port of Rayforge's `GrblSerialDriver` against
//! an async byte [`Transport`].  All Python-facing notifications go
//! through the [`SessionEvents`] trait so the core stays pyo3-free.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::dialect::{format_template, Arg, GrblDialect};
use super::flow::FlowControl;
use super::flow::GrblResponseType;
use super::parser;
use super::transport::mock::MockInner;
use super::transport::serial::SerialTransport;
use super::transport::telnet::TelnetTransport;
use super::transport::{Transport, TransportError};
use super::types::{DeviceState, DeviceStatus, TransportStatus, UnitSystem};

tokio::task_local! {
    /// Set inside the streaming task so `abort_stream_task` can
    /// detect self-interruption (Python: `task is
    /// asyncio.current_task()`).
    static IN_STREAM_TASK: bool;
}

/// Notifications emitted by the session, implemented by the Python
/// bindings (or by embedders).
pub trait SessionEvents: Send + Sync {
    fn state_changed(&self, state: &DeviceState);
    fn connection_status(&self, status: TransportStatus, message: Option<&str>);
    fn command_status(&self, status: TransportStatus, message: Option<&str>);
    fn job_finished(&self);
    fn probe_status(&self, message: &str);
    fn wcs_updated(&self, offsets: &HashMap<String, (f64, f64, f64)>);
    fn config_changed(&self, key: &str, value: i64);
    fn command_done(&self, op_index: i64);
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

/// Which physical transport to use.
#[derive(Clone)]
pub enum TransportKind {
    Serial { port: String, baudrate: u32 },
    Telnet { host: String, port: u16 },
    Mock(Arc<MockInner>),
}

/// Session configuration with the Python driver's default timings.
#[derive(Clone)]
pub struct SessionConfig {
    pub transport: TransportKind,
    pub poll_status_while_running: bool,
    pub deadlock_detection: bool,
    pub rx_buffer_size_override: i64,
    pub cached_rx_buffer_size: Option<i64>,
    pub stall_timeout_min: f64,
    pub stall_timeout_max: f64,
    pub stall_timeout_safety_factor: f64,
    pub stall_timeout_default: f64,
    pub safety_shutdown_delay: f64,
    pub unanswered_poll_limit: u32,
    pub poll_response_attempts: u32,
    pub poll_response_interval: f64,
    pub handshake_timeout: f64,
    pub handshake_poll_interval: f64,
    pub status_poll_interval: f64,
    pub reconnect_delay: f64,
    pub command_timeout: f64,
}

impl SessionConfig {
    pub fn new(transport: TransportKind) -> Self {
        Self {
            transport,
            poll_status_while_running: false,
            deadlock_detection: false,
            rx_buffer_size_override: 0,
            cached_rx_buffer_size: None,
            stall_timeout_min: 5.0,
            stall_timeout_max: 120.0,
            stall_timeout_safety_factor: 3.0,
            stall_timeout_default: 30.0,
            safety_shutdown_delay: 0.2,
            unanswered_poll_limit: 3,
            poll_response_attempts: 10,
            poll_response_interval: 0.1,
            handshake_timeout: 6.0,
            handshake_poll_interval: 0.5,
            status_poll_interval: 0.5,
            reconnect_delay: 5.0,
            command_timeout: 10.0,
        }
    }

    fn secs(v: f64) -> Duration {
        Duration::from_secs_f64(v.max(0.0))
    }
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

pub(crate) struct RequestShared {
    pub command: String,
    response_lines: Mutex<Vec<String>>,
    finished: AsyncEvent,
}

impl RequestShared {
    fn new(command: &str) -> Self {
        Self {
            command: command.to_string(),
            response_lines: Mutex::new(Vec::new()),
            finished: AsyncEvent::new(),
        }
    }

    fn add_line(&self, line: &str) {
        self.response_lines.lock().unwrap().push(line.to_string());
    }

    fn finish(&self) {
        self.finished.set();
    }

    fn is_finished(&self) -> bool {
        self.finished.is_set()
    }

    fn lines(&self) -> Vec<String> {
        self.response_lines.lock().unwrap().clone()
    }
}

struct QueuedCommand {
    shared: Arc<RequestShared>,
}

/// Completion tracking for the streaming task.  The guard's `Drop`
/// marks completion even when the task is hard-aborted (a dropped
/// future cannot run cleanup code).
struct StreamDone {
    finished: AtomicBool,
    notify: tokio::sync::Notify,
}

impl StreamDone {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            finished: AtomicBool::new(false),
            notify: tokio::sync::Notify::new(),
        })
    }
}

struct StreamGuard {
    done: Arc<StreamDone>,
}

impl Drop for StreamGuard {
    fn drop(&mut self) {
        self.done.finished.store(true, Ordering::SeqCst);
        self.done.notify.notify_waiters();
        self.done.notify.notify_one();
    }
}

/// Handle for the running streaming task: abort + reap support
/// without holding the `JoinHandle` (which the caller awaits).
struct StreamHandle {
    abort: tokio::task::AbortHandle,
    done: Arc<StreamDone>,
}

impl StreamHandle {
    fn is_finished(&self) -> bool {
        self.done.finished.load(Ordering::SeqCst)
    }

    async fn abort_and_wait(&self) {
        if self.is_finished() {
            return;
        }
        self.abort.abort();
        loop {
            if self.is_finished() {
                return;
            }
            let notified = self.done.notify.notified();
            if self.is_finished() {
                return;
            }
            notified.await;
        }
    }
}

/// Shared session state, owned jointly by the public handle and the
/// background tasks.
pub(crate) struct SessionCore {
    config: SessionConfig,
    dialect: RwLock<GrblDialect>,
    events: Arc<dyn SessionEvents>,
    transport: Arc<dyn Transport>,
    flow: FlowControl,
    state: Mutex<DeviceState>,
    keep_running: AtomicBool,
    job_running: AtomicBool,
    is_cancelled: AtomicBool,
    is_holding: AtomicBool,
    raw_grbl_status: Mutex<DeviceStatus>,
    device_response_count: AtomicU64,
    consecutive_unanswered_polls: AtomicU32,
    job_exception: Mutex<Option<SessionError>>,
    current_request: Mutex<Option<Arc<RequestShared>>>,
    interactive_request: Mutex<Option<Arc<RequestShared>>>,
    queued_requests: Mutex<Vec<std::sync::Weak<RequestShared>>>,
    cmd_tx: Mutex<mpsc::UnboundedSender<QueuedCommand>>,
    handshake_received: AsyncEvent,
    last_connection_status: Mutex<TransportStatus>,
    report_in_inches: AtomicBool,
    last_reported_op_index: Mutex<i64>,
    stream_task: Mutex<Option<StreamHandle>>,
    connection_task: Mutex<Option<JoinHandle<()>>>,
    command_task: Mutex<Option<JoinHandle<()>>>,
    reader_task: Mutex<Option<JoinHandle<()>>>,
    cmd_lock: tokio::sync::Mutex<()>,
}

impl SessionCore {
    pub(crate) fn new(
        config: SessionConfig,
        dialect: GrblDialect,
        events: Arc<dyn SessionEvents>,
    ) -> Arc<Self> {
        let transport: Arc<dyn Transport> = match &config.transport {
            TransportKind::Serial { port, baudrate } => {
                SerialTransport::new(port, *baudrate)
            }
            TransportKind::Telnet { host, port } => {
                TelnetTransport::new(host, *port)
            }
            TransportKind::Mock(inner) => inner.clone(),
        };
        let (cmd_tx, _) = mpsc::unbounded_channel();
        let core = Self {
            config,
            dialect: RwLock::new(dialect),
            events,
            transport,
            flow: FlowControl::new(),
            state: Mutex::new(DeviceState::default()),
            keep_running: AtomicBool::new(false),
            job_running: AtomicBool::new(false),
            is_cancelled: AtomicBool::new(false),
            is_holding: AtomicBool::new(false),
            raw_grbl_status: Mutex::new(DeviceStatus::Unknown),
            device_response_count: AtomicU64::new(0),
            consecutive_unanswered_polls: AtomicU32::new(0),
            job_exception: Mutex::new(None),
            current_request: Mutex::new(None),
            interactive_request: Mutex::new(None),
            queued_requests: Mutex::new(Vec::new()),
            cmd_tx: Mutex::new(cmd_tx),
            handshake_received: AsyncEvent::new(),
            last_connection_status: Mutex::new(TransportStatus::Unknown),
            report_in_inches: AtomicBool::new(false),
            last_reported_op_index: Mutex::new(-1),
            stream_task: Mutex::new(None),
            connection_task: Mutex::new(None),
            command_task: Mutex::new(None),
            reader_task: Mutex::new(None),
            cmd_lock: tokio::sync::Mutex::new(()),
        };
        if let Some(cached) = core.config.cached_rx_buffer_size {
            if cached > 0 {
                core.flow.set_rx_buffer_size(cached as usize);
            }
        }
        Arc::new(core)
    }

    pub(crate) fn state(&self) -> DeviceState {
        self.state.lock().unwrap().clone()
    }

    pub(crate) fn update_dialect(&self, dialect: GrblDialect) {
        *self.dialect.write().unwrap() = dialect;
    }

    fn dialect(&self) -> GrblDialect {
        self.dialect.read().unwrap().clone()
    }

    pub(crate) fn is_job_running(&self) -> bool {
        self.job_running.load(Ordering::SeqCst)
    }

    fn is_cancelled(&self) -> bool {
        self.is_cancelled.load(Ordering::SeqCst)
    }

    fn is_holding(&self) -> bool {
        self.is_holding.load(Ordering::SeqCst)
    }

    fn transport_open(&self) -> Result<Arc<dyn Transport>, SessionError> {
        if !self.transport.is_open() {
            return Err(SessionError::Connection(
                "Serial transport not connected".to_string(),
            ));
        }
        Ok(self.transport.clone())
    }

    pub(crate) fn resource_uri(&self) -> Option<String> {
        self.transport.resource_uri()
    }

    pub(crate) fn buffer_count(&self) -> usize {
        self.flow.buffer_count()
    }

    pub(crate) fn rx_buffer_size(&self) -> usize {
        self.flow.rx_buffer_size()
    }

    pub(crate) fn pending_commands(&self) -> Vec<String> {
        self.flow
            .pending_queue()
            .iter()
            .map(|p| p.command.clone())
            .collect()
    }

    fn note_device_response(&self) {
        self.device_response_count.fetch_add(1, Ordering::SeqCst);
    }

    fn device_response_count(&self) -> u64 {
        self.device_response_count.load(Ordering::SeqCst)
    }

    fn update_connection_status(
        &self,
        status: TransportStatus,
        message: Option<&str>,
    ) {
        *self.last_connection_status.lock().unwrap() = status;
        self.events.connection_status(status, message);
    }

    // ---------------------------------------------------------------
    // Connect / disconnect
    // ---------------------------------------------------------------

    pub(crate) async fn connect(self: &Arc<Self>) {
        self.abort_background_tasks().await;

        self.keep_running.store(true, Ordering::SeqCst);
        self.is_cancelled.store(false, Ordering::SeqCst);
        self.job_running.store(false, Ordering::SeqCst);
        self.job_exception.lock().unwrap().take();
        self.flow.reset();

        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        *self.cmd_tx.lock().unwrap() = cmd_tx;

        let core = self.clone();
        let connection_task =
            tokio::spawn(async move { core.connection_loop().await });
        *self.connection_task.lock().unwrap() = Some(connection_task);

        let core = self.clone();
        let command_task = tokio::spawn(async move {
            core.command_queue_loop(cmd_rx).await;
        });
        *self.command_task.lock().unwrap() = Some(command_task);
    }

    async fn abort_background_tasks(&self) {
        for slot in [&self.connection_task, &self.command_task] {
            let task = slot.lock().unwrap().take();
            if let Some(task) = task {
                task.abort();
                let _ = task.await;
            }
        }
        let reader = self.reader_task.lock().unwrap().take();
        if let Some(task) = reader {
            task.abort();
        }
        self.transport.close().await;
    }

    pub(crate) async fn cleanup(&self) {
        self.keep_running.store(false, Ordering::SeqCst);
        self.job_running.store(false, Ordering::SeqCst);
        self.is_cancelled.store(true, Ordering::SeqCst);
        self.abort_stream_task().await;
        self.is_cancelled.store(false, Ordering::SeqCst);
        self.flow.reset();
        self.job_exception.lock().unwrap().take();
        self.abort_background_tasks().await;
        self.update_connection_status(TransportStatus::Disconnected, None);
    }

    async fn await_handshake(&self) -> Result<bool, SessionError> {
        let deadline = tokio::time::Instant::now()
            + SessionConfig::secs(self.config.handshake_timeout);
        loop {
            self.transport.write(b"?").await?;
            let poll_deadline = tokio::time::Instant::now()
                + SessionConfig::secs(self.config.handshake_poll_interval);
            tokio::select! {
                _ = self.handshake_received.wait() => return Ok(true),
                _ = tokio::time::sleep_until(poll_deadline) => {
                    if tokio::time::Instant::now() >= deadline {
                        return Ok(false);
                    }
                }
            }
        }
    }

    async fn connection_loop(self: Arc<Self>) {
        log::debug!("Entering connection loop.");
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
                *self.reader_task.lock().unwrap() = Some(reader);

                match self.await_handshake().await {
                    Ok(true) => {
                        log::info!("Connection established successfully.");
                        self.apply_cached_rx_buffer_size();
                        if let Err(err) = self
                            .execute_interactive_command("$I".to_string())
                            .await
                        {
                            log::warn!("Failed to retrieve build info: {err}");
                        }
                        self.warn_if_buffer_size_unknown();
                        self.update_connection_status(
                            TransportStatus::Connected,
                            None,
                        );
                        self.status_poll_loop().await;
                    }
                    Ok(false) => {
                        log::warn!(
                            "No response from device. Port may be a \
                             phantom COM port without a connected device."
                        );
                        self.update_connection_status(
                            TransportStatus::Error,
                            Some("No response from device"),
                        );
                    }
                    Err(err) => {
                        log::error!("Connection error: {err}");
                        self.update_connection_status(
                            TransportStatus::Error,
                            Some(&err.to_string()),
                        );
                    }
                }

                if let Some(task) = self.reader_task.lock().unwrap().take() {
                    task.abort();
                }
                self.transport.close().await;
            }

            if !self.keep_running.load(Ordering::SeqCst) {
                break;
            }
            log::debug!("Connection lost. Reconnecting.");
            self.update_connection_status(TransportStatus::Sleeping, None);
            tokio::time::sleep(SessionConfig::secs(
                self.config.reconnect_delay,
            ))
            .await;
        }
        log::debug!("Leaving connection loop.");
    }

    async fn status_poll_loop(&self) {
        while self.transport.is_open()
            && self.keep_running.load(Ordering::SeqCst)
        {
            if !self.config.poll_status_while_running
                && self.job_running.load(Ordering::SeqCst)
            {
                tokio::time::sleep(SessionConfig::secs(
                    self.config.status_poll_interval,
                ))
                .await;
                continue;
            }

            // Deliberately not taking cmd_lock here: a gcode send
            // holds the lock for the whole buffer-space wait.  Polls
            // are realtime bytes that bypass the GRBL RX buffer and
            // its accounting, so they are safe to send without the
            // lock.  Blocking on the lock here would starve status
            // polling, so the driver state would go stale.
            let responses_before = self.device_response_count();
            if let Err(err) = self.transport.write(b"?").await {
                log::warn!("Connection lost while sending poll: {err}");
                break;
            }
            tokio::time::sleep(SessionConfig::secs(
                self.config.status_poll_interval,
            ))
            .await;

            if !self.keep_running.load(Ordering::SeqCst)
                || !self.transport.is_open()
            {
                break;
            }

            // A device that answers polls after a transient write
            // error is alive: recover the connection status instead
            // of leaving the UI stuck in ERROR (issue #428).
            if *self.last_connection_status.lock().unwrap()
                == TransportStatus::Error
                && self.device_response_count() > responses_before
            {
                log::info!(
                    "Device responded after connection error; \
                     recovering connection status."
                );
                self.update_connection_status(TransportStatus::Connected, None);
            }
        }
    }

    // ---------------------------------------------------------------
    // RX handling
    // ---------------------------------------------------------------

    fn handle_rx(&self, data: &[u8]) {
        let responses = self.flow.parse_incoming(data);

        // Process LINE responses before OK/ERROR to ensure
        // informational lines are collected before the command is
        // marked as finished.
        let mut lines = Vec::new();
        let mut acks = Vec::new();
        for resp in responses {
            match resp.rtype {
                GrblResponseType::Line => lines.push(resp),
                _ => acks.push(resp),
            }
        }
        for resp in lines.into_iter().chain(acks) {
            match resp.rtype {
                GrblResponseType::Ok => self.handle_ok(resp.pending),
                GrblResponseType::Error => self.handle_error(&resp.text),
                GrblResponseType::Line => self.handle_line(&resp.text),
            }
        }
        // Wake parked senders only after the responses were
        // processed, so progress callbacks and request completions
        // are always observed first (asyncio-like ordering).
        self.flow.flush_notifications();
    }

    fn handle_ok(&self, pending: Option<super::flow::PendingCommand>) {
        self.note_device_response();

        // Logic for single, interactive commands.
        let request = self
            .interactive_request
            .lock()
            .unwrap()
            .clone()
            .or_else(|| self.current_request.lock().unwrap().clone());
        if let Some(request) = request {
            if !request.is_finished() {
                request.add_line("ok");
                self.events.command_status(TransportStatus::Idle, None);
                log::debug!(
                    "Command {:?} completed with 'ok'",
                    request.command
                );
                request.finish();
            }
        }

        // Logic for streaming protocol during a job.
        if self.job_running.load(Ordering::SeqCst) {
            if let Some(op_index) = pending.as_ref().and_then(|p| p.op_index) {
                let mut last = self.last_reported_op_index.lock().unwrap();
                for i in (*last + 1)..=op_index {
                    log::debug!("Firing on_command_done for op_index {i}");
                    self.events.command_done(i);
                }
                *last = op_index;
            }
        }
    }

    fn handle_error(&self, text: &str) {
        self.note_device_response();
        let error_code = text.split(':').nth(1).unwrap_or("").trim();
        let error = parser::get_error(error_code);
        let state_snapshot = {
            let mut state = self.state.lock().unwrap();
            state.error = Some(error);
            state.clone()
        };
        // Emit outside the lock: event delivery attaches to the
        // Python GIL and must never run while holding a lock that
        // Python-side code could take.
        self.events.state_changed(&state_snapshot);

        let request = self
            .interactive_request
            .lock()
            .unwrap()
            .clone()
            .or_else(|| self.current_request.lock().unwrap().clone());
        if let Some(request) = request {
            if !request.is_finished() {
                request.add_line(text);
                self.events
                    .command_status(TransportStatus::Error, Some(text));
                request.finish();
            }
        }

        if self.job_running.load(Ordering::SeqCst) {
            self.events
                .command_status(TransportStatus::Error, Some(text));
            log::error!("GRBL error during job: {text}. Halting stream.");
            *self.job_exception.lock().unwrap() =
                Some(SessionError::Device(format!("GRBL error: {text}")));
            self.flow.signal_space_available();
        }
    }

    fn handle_line(&self, line: &str) {
        if line.starts_with('<') && !line.ends_with('>') {
            log::debug!("Ignoring fragmented status report: {line}");
            return;
        }
        if line.contains("Pos:") && line.contains('|') && !line.starts_with('<')
        {
            log::debug!("Ignoring fragmented status report: {line}");
            return;
        }
        if line.starts_with('<') && line.ends_with('>') {
            self.handshake_received.set();
            self.handle_status_report(line.trim());
            return;
        }

        // Collect response lines for pending single commands.
        let request = self
            .interactive_request
            .lock()
            .unwrap()
            .clone()
            .or_else(|| self.current_request.lock().unwrap().clone());
        if let Some(request) = request {
            if !request.is_finished() {
                request.add_line(line);
            }
        }

        if let Some(code) = line.strip_prefix("ALARM:") {
            let code = code.trim();
            let error = super::errors::alarm_code_to_device_error(code);
            let state_snapshot = {
                let mut state = self.state.lock().unwrap();
                state.error = Some(error);
                state.clone()
            };
            // Emit outside the lock (see handle_error).
            self.events.state_changed(&state_snapshot);
            self.events
                .command_status(TransportStatus::Error, Some(line));
            if self.job_running.load(Ordering::SeqCst) {
                log::error!("GRBL ALARM during job: {line}. Halting stream.");
                *self.job_exception.lock().unwrap() =
                    Some(SessionError::Device(format!("GRBL ALARM: {line}")));
                self.flow.signal_space_available();
            }
        } else if line.starts_with("[VER:") {
            if let Some(ver) = parser::parse_version(&[line.to_string()]) {
                log::info!("Connected to GRBL version {ver}");
            }
        } else if line.starts_with("[OPT:") {
            if let Some(rx_buffer_size) = parser::parse_opt_info(line) {
                self.cache_rx_buffer_size(rx_buffer_size);
                if self.config.rx_buffer_size_override <= 0 {
                    self.flow.set_rx_buffer_size(rx_buffer_size as usize);
                }
            }
        } else if line.starts_with("Grbl ") || line.starts_with("GrblHAL ") {
            self.handshake_received.set();
            log::debug!("Received Grbl welcome message: {line}");
        } else {
            log::debug!("Received informational line: {line}");
        }
    }

    fn handle_status_report(&self, report: &str) {
        self.note_device_response();
        let report_in_inches = self.report_in_inches.load(Ordering::SeqCst);

        let mut state = {
            let current = self.state.lock().unwrap();
            parser::parse_state(report, &current, report_in_inches)
        };

        *self.raw_grbl_status.lock().unwrap() = state.status;

        if let Some(total) = state.buffer_rx_available {
            if state.status == DeviceStatus::Idle
                && self.config.rx_buffer_size_override <= 0
            {
                let total = total as usize;
                let current = self.flow.rx_buffer_size();
                if total > 0 && total != current {
                    log::info!(
                        "Detected RX buffer size {total} from Bf: status \
                         field (device idle, available == total)"
                    );
                    self.flow.set_rx_buffer_size(total);
                    self.cache_rx_buffer_size(total as i64);
                }
            }
        }

        // If a job is active, 'Idle' state between commands should be
        // reported as 'Run' to the UI.
        if self.job_running.load(Ordering::SeqCst)
            && state.status == DeviceStatus::Idle
        {
            state.status = DeviceStatus::Run;
        }

        // The driver owns the pause state: while a hold was
        // requested, force HOLD so a firmware status report cannot
        // mask it.
        if self.is_holding() && state.status != DeviceStatus::Alarm {
            state.status = DeviceStatus::Hold;
        }

        let changed = {
            let mut current = self.state.lock().unwrap();
            if state != *current {
                let old_status = current.status;
                *current = state.clone();
                if state.status != old_status {
                    log::debug!("Device state changed: {}", state.status);
                }
                true
            } else {
                false
            }
        };
        // Emit outside the lock (see handle_error).
        if changed {
            self.events.state_changed(&state);
        }
    }

    fn apply_cached_rx_buffer_size(&self) {
        if self.config.rx_buffer_size_override > 0 {
            log::info!(
                "Applying RX buffer size override: {} bytes",
                self.config.rx_buffer_size_override
            );
            self.flow.set_rx_buffer_size(
                self.config.rx_buffer_size_override as usize,
            );
            return;
        }
        if let Some(cached) = self.config.cached_rx_buffer_size {
            if cached > 0 {
                log::info!("Applying cached RX buffer size: {cached} bytes");
                self.flow.set_rx_buffer_size(cached as usize);
            }
        }
    }

    fn cache_rx_buffer_size(&self, size: i64) {
        log::info!("Caching RX buffer size: {size} bytes");
        self.events.config_changed("rx_buffer_size", size);
    }

    fn warn_if_buffer_size_unknown(&self) {
        if self.config.rx_buffer_size_override > 0 {
            return;
        }
        if self.config.cached_rx_buffer_size.is_some() {
            return;
        }
        log::warn!(
            "Device did not report RX buffer size via $I. Using \
             default {} bytes.",
            super::flow::DEFAULT_GRBL_RX_BUFFER_SIZE
        );
    }

    // ---------------------------------------------------------------
    // Command execution
    // ---------------------------------------------------------------

    pub(crate) async fn execute_interactive_command(
        &self,
        command: String,
    ) -> Result<Vec<String>, SessionError> {
        self.transport_open()?;
        let request = Arc::new(RequestShared::new(&command));
        {
            let _guard = self.cmd_lock.lock().await;
            let _ = self.flow.wait_pending_empty(None).await;
            *self.interactive_request.lock().unwrap() = Some(request.clone());
            let result = async {
                self.send_command_bytes(format!("{command}\n").as_bytes())
                    .await?;
                match tokio::time::timeout(
                    SessionConfig::secs(self.config.command_timeout),
                    request.finished.wait(),
                )
                .await
                {
                    Ok(()) => Ok(()),
                    Err(_) => Err(SessionError::Timeout),
                }
            }
            .await;
            *self.interactive_request.lock().unwrap() = None;
            result?;
        }
        Ok(request.lines())
    }

    /// Send a command through the queued command processor and await
    /// its full response.
    pub(crate) async fn execute_command(
        &self,
        command: String,
    ) -> Result<Vec<String>, SessionError> {
        // Only clear the cancellation flag once the cancelled job's
        // sender has fully terminated (issue #428).
        if !self.job_running.load(Ordering::SeqCst) {
            let done = self
                .stream_task
                .lock()
                .unwrap()
                .as_ref()
                .is_none_or(|t| t.is_finished());
            if done {
                self.is_cancelled.store(false, Ordering::SeqCst);
            }
        }
        let request = Arc::new(RequestShared::new(&command));
        self.queued_requests
            .lock()
            .unwrap()
            .push(Arc::downgrade(&request));
        self.cmd_tx
            .lock()
            .unwrap()
            .send(QueuedCommand {
                shared: request.clone(),
            })
            .map_err(|_| {
                SessionError::Connection(
                    "command queue unavailable".to_string(),
                )
            })?;
        match tokio::time::timeout(
            SessionConfig::secs(self.config.command_timeout),
            request.finished.wait(),
        )
        .await
        {
            Ok(()) => Ok(request.lines()),
            Err(_) => {
                request.finish();
                Err(SessionError::Timeout)
            }
        }
    }

    async fn command_queue_loop(
        self: Arc<Self>,
        mut rx: mpsc::UnboundedReceiver<QueuedCommand>,
    ) {
        log::debug!("Entering command queue loop.");
        while self.keep_running.load(Ordering::SeqCst) {
            let Some(msg) = rx.recv().await else { break };
            let request = msg.shared;
            {
                let mut queued = self.queued_requests.lock().unwrap();
                queued.retain(|weak| {
                    weak.upgrade()
                        .is_none_or(|arc| !Arc::ptr_eq(&arc, &request))
                });
            }
            if !self.transport.is_open() || self.is_cancelled() {
                log::warn!(
                    "Cannot process command: transport not connected or \
                     job is cancelled. Dropping command."
                );
                request.finish();
                continue;
            }
            *self.current_request.lock().unwrap() = Some(request.clone());
            let payload = format!("{}\n", request.command);
            let result = {
                let _guard = self.cmd_lock.lock().await;
                if !self.transport.is_open() {
                    Err(SessionError::Connection(
                        "Serial transport disconnected during command."
                            .to_string(),
                    ))
                } else {
                    self.send_command_bytes(payload.as_bytes()).await
                }
            };
            match result {
                Err(err) => {
                    log::error!("Connection error during command: {err}");
                    self.update_connection_status(
                        TransportStatus::Error,
                        Some(&err.to_string()),
                    );
                }
                Ok(()) => {
                    request.finished.wait().await;
                }
            }
            *self.current_request.lock().unwrap() = None;
            // Release lock briefly to allow status polling.
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        log::debug!("Leaving command queue loop.");
    }

    /// Send an interactive command ($$, $G, …) with buffer
    /// accounting: wait up to 1 s for space, then send regardless.
    async fn send_command_bytes(
        &self,
        payload: &[u8],
    ) -> Result<(), SessionError> {
        let _ = self
            .flow
            .wait_for_space(payload.len(), Duration::from_secs(1))
            .await;
        let text = String::from_utf8_lossy(payload).to_string();
        self.flow
            .track_command(payload.len(), None, text.trim_end());
        self.transport.write(payload).await?;
        Ok(())
    }

    async fn send_realtime(
        &self,
        command: &str,
        add_newline: bool,
    ) -> Result<(), SessionError> {
        let transport = self.transport_open()?;
        let mut payload = command.to_string();
        if add_newline {
            payload.push('\n');
        }
        log::debug!("Sending realtime command: {command}");
        transport.write(payload.as_bytes()).await?;
        Ok(())
    }

    // ---------------------------------------------------------------
    // Job streaming
    // ---------------------------------------------------------------

    async fn start_job(&self) {
        self.is_cancelled.store(false, Ordering::SeqCst);
        self.job_running.store(true, Ordering::SeqCst);
        self.is_holding.store(false, Ordering::SeqCst);
        *self.last_reported_op_index.lock().unwrap() = -1;
        self.job_exception.lock().unwrap().take();
        self.consecutive_unanswered_polls.store(0, Ordering::SeqCst);
        self.flow.reset_flow_control();

        // The driver owns the state while a job runs: status polling
        // is disabled during jobs by default, so reflect the job
        // start immediately so the UI does not keep showing Idle.
        // An ALARM (which aborts the job right away) is not masked.
        let state_snapshot = {
            let mut state = self.state.lock().unwrap();
            if state.status != DeviceStatus::Run
                && state.status != DeviceStatus::Alarm
            {
                state.status = DeviceStatus::Run;
                Some(state.clone())
            } else {
                None
            }
        };
        // Emit outside the lock (see handle_error).
        if let Some(state_snapshot) = state_snapshot {
            self.events.state_changed(&state_snapshot);
        }
    }

    /// Cancel and reap the streaming task, if one is still alive.
    ///
    /// Self-interruption (called from within the streaming task as
    /// part of the interrupt handler) returns immediately: cancelling
    /// the current task here would cut the ongoing cancel() —
    /// including the safety shutdown — short.
    async fn abort_stream_task(&self) {
        if IN_STREAM_TASK.try_with(|v| *v).unwrap_or(false) {
            return;
        }
        let task = self.stream_task.lock().unwrap().take();
        if let Some(task) = task {
            task.abort_and_wait().await;
        }
    }

    async fn run_streaming_job<F>(&self, fut: F) -> Result<(), SessionError>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.abort_stream_task().await;
        let done = StreamDone::new();
        let guard = StreamGuard { done: done.clone() };
        let task = tokio::spawn(IN_STREAM_TASK.scope(true, async move {
            let _guard = guard;
            fut.await;
        }));
        *self.stream_task.lock().unwrap() = Some(StreamHandle {
            abort: task.abort_handle(),
            done: done.clone(),
        });
        let result = match task.await {
            Ok(()) => Ok(()),
            Err(err) if err.is_cancelled() => Ok(()),
            Err(err) => Err(SessionError::Other(err.to_string())),
        };
        let mut slot = self.stream_task.lock().unwrap();
        if slot.as_ref().is_some_and(|h| Arc::ptr_eq(&h.done, &done)) {
            *slot = None;
        }
        result
    }

    pub(crate) async fn run(
        self: &Arc<Self>,
        gcode: &str,
        op_map: HashMap<usize, i64>,
        op_estimates: Vec<f64>,
    ) {
        self.start_job().await;
        let lines: Vec<String> = gcode.lines().map(|l| l.to_string()).collect();
        let core = self.clone();
        if let Err(err) = self
            .run_streaming_job(async move {
                core.stream_gcode(lines, Some(op_map), Some(op_estimates))
                    .await;
            })
            .await
        {
            match err {
                SessionError::Device(msg) => log::warn!(
                    "Job terminated due to device error: {msg}. \
                     Connection remains active."
                ),
                other => {
                    log::error!(
                        "Job terminated with unexpected error: {other}"
                    );
                }
            }
        }
    }

    pub(crate) async fn run_raw(self: &Arc<Self>, machine_code: &str) {
        let lines: Vec<String> = machine_code
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        let (gcode, realtime) = parser::split_realtime_commands(&lines);
        for line in &realtime {
            if let Err(err) = self.send_realtime(line, false).await {
                log::error!("Realtime command failed: {err}");
                return;
            }
        }
        if gcode.is_empty() {
            return;
        }
        self.start_job().await;
        let core = self.clone();
        if let Err(err) = self
            .run_streaming_job(async move {
                core.stream_gcode(gcode, None, None).await;
            })
            .await
        {
            log::error!("Raw G-code terminated with error: {err}");
        }
    }

    async fn stream_gcode(
        self: Arc<Self>,
        gcode_lines: Vec<String>,
        op_map: Option<HashMap<usize, i64>>,
        command_times: Option<Vec<f64>>,
    ) {
        let total = gcode_lines.len();
        log::debug!("Starting GRBL streaming job with {total} lines.");
        let mut sent_count = 0usize;
        let outcome = self
            .stream_gcode_inner(
                &gcode_lines,
                op_map.as_ref(),
                command_times.as_deref(),
                &mut sent_count,
            )
            .await;

        // Interrupt handling (Python: the except clause).  Runs
        // before the job-state resets so cancel() still sees the job
        // as running and emits job_finished.
        if let Err(ref err) = outcome {
            if !self.is_cancelled() {
                log::warn!("Job interrupted: {err:?}; calling cancel()");
                let _ = self.cancel(true).await;
            }
        }

        // Finally-block equivalent.  Hard aborts (task abort) never
        // get here; cancel() performs its own cleanup.
        *self.raw_grbl_status.lock().unwrap() = DeviceStatus::Unknown;
        self.job_running.store(false, Ordering::SeqCst);
        self.is_holding.store(false, Ordering::SeqCst);

        match outcome {
            Ok(()) if !self.is_cancelled() => {
                self.events.job_finished();
                log::debug!(
                    "G-code streaming finished successfully \
                     ({sent_count}/{total} lines)."
                );
            }
            Ok(()) => {
                log::debug!(
                    "G-code streaming cancelled at line \
                     {sent_count}/{total}."
                );
            }
            Err(_) => {
                log::debug!(
                    "G-code streaming aborted at line \
                     {sent_count}/{total}."
                );
            }
        }
    }

    async fn stream_gcode_inner(
        &self,
        gcode_lines: &[String],
        op_map: Option<&HashMap<usize, i64>>,
        command_times: Option<&[f64]>,
        sent_count: &mut usize,
    ) -> Result<(), SessionError> {
        for (line_idx, raw_line) in gcode_lines.iter().enumerate() {
            if self.is_cancelled()
                || self.job_exception.lock().unwrap().is_some()
                || self.state.lock().unwrap().status == DeviceStatus::Alarm
            {
                log::info!(
                    "Job cancelled, errored, or machine in ALARM \
                     state. Stopping G-code sending."
                );
                let status_is_alarm =
                    self.state.lock().unwrap().status == DeviceStatus::Alarm;
                let mut exception = self.job_exception.lock().unwrap();
                if status_is_alarm && exception.is_none() {
                    *exception = Some(SessionError::Device(
                        "Machine entered ALARM state during job.".to_string(),
                    ));
                }
                break;
            }

            let line = parser::strip_gcode_comments(raw_line);
            if line.is_empty() {
                continue;
            }

            let op_index = op_map.and_then(|m| m.get(&line_idx).copied());
            let command_bytes = format!("{line}\n");

            let timeout = match (command_times, op_index) {
                (Some(times), Some(op)) if (op as usize) < times.len() => {
                    let estimated = times[op as usize];
                    (estimated * self.config.stall_timeout_safety_factor).clamp(
                        self.config.stall_timeout_min,
                        self.config.stall_timeout_max,
                    )
                }
                _ => self.config.stall_timeout_default,
            };

            match self
                .send_gcode_line(
                    command_bytes.as_bytes(),
                    op_index,
                    SessionConfig::secs(timeout),
                )
                .await
            {
                Ok(()) => {}
                Err(SessionError::BufferStall(msg)) => {
                    let mut exception = self.job_exception.lock().unwrap();
                    if exception.is_none() {
                        *exception = Some(SessionError::Device(format!(
                            "Deadlock recovery failed: {msg}"
                        )));
                    }
                }
                Err(err) => return Err(err),
            }
            if self.job_exception.lock().unwrap().is_some()
                || self.state.lock().unwrap().status == DeviceStatus::Alarm
            {
                break;
            }

            *sent_count += 1;
            if (*sent_count).is_multiple_of(500) {
                log::debug!(
                    "Streaming progress: {}/{} lines sent",
                    sent_count,
                    gcode_lines.len()
                );
            }
            tokio::task::yield_now().await;
        }

        if !self.is_cancelled() && self.job_exception.lock().unwrap().is_none()
        {
            log::debug!("All G-code sent. Waiting for all 'ok' responses.");
            self.drain_pending_acks(SessionConfig::secs(
                self.config.stall_timeout_default,
            ))
            .await;
        }

        if let Some(exception) = self.job_exception.lock().unwrap().take() {
            return Err(exception);
        }
        Ok(())
    }

    async fn send_gcode_line(
        &self,
        command_bytes: &[u8],
        op_index: Option<i64>,
        timeout: Duration,
    ) -> Result<(), SessionError> {
        let _guard = self.cmd_lock.lock().await;
        if !self.transport.is_open() {
            return Err(SessionError::Connection(
                "Serial transport disconnected during job.".to_string(),
            ));
        }
        self.send_gcode_with_stall(command_bytes, op_index, timeout)
            .await
    }

    /// Send a single gcode line with buffer accounting and the
    /// stall/liveness callback.
    async fn send_gcode_with_stall(
        &self,
        data: &[u8],
        op_index: Option<i64>,
        timeout: Duration,
    ) -> Result<(), SessionError> {
        let command_len = data.len();
        loop {
            let waited = self.flow.wait_for_space(command_len, timeout).await;
            match waited {
                Ok(()) => break,
                Err(_) => {
                    log::warn!(
                        "Buffer stall: timed out waiting for \
                         {command_len} bytes. Invoking recovery callback."
                    );
                    // The caller holds cmd_lock here (we are called
                    // from send_gcode_line), so the stall machinery
                    // must not take the lock itself.
                    let handled = self.on_buffer_stall(command_len).await?;
                    if handled {
                        log::info!("Recovery successful, retrying send.");
                        continue;
                    }
                    return Err(SessionError::BufferStall(format!(
                        "cannot get {command_len} bytes of space"
                    )));
                }
            }
        }
        let text = String::from_utf8_lossy(data).to_string();
        self.flow
            .track_command(command_len, op_index, text.trim_end());
        self.transport.write(data).await?;
        Ok(())
    }

    /// Stall callback for gcode sends: abort the job only with proof
    /// that it cannot proceed (cancelled, ALARM, or a device that
    /// stopped responding).  A device that answers polls is alive,
    /// so the wait is retried.
    ///
    /// The caller holds `cmd_lock`.
    async fn on_buffer_stall(
        &self,
        command_len: usize,
    ) -> Result<bool, SessionError> {
        if self.is_cancelled() {
            return Ok(false);
        }

        let idle_or_desynced = self.poll_and_check_idle(false).await;

        if self.device_stopped_responding() {
            self.mark_device_unresponsive();
            return Ok(false);
        }

        if self.state.lock().unwrap().status == DeviceStatus::Alarm {
            let mut exception = self.job_exception.lock().unwrap();
            if exception.is_none() {
                *exception = Some(SessionError::Device(
                    "Machine entered ALARM state during job.".to_string(),
                ));
            }
            return Ok(false);
        }

        if !self.config.deadlock_detection {
            log::debug!(
                "Buffer stall timed out (deadlock detection disabled). \
                 Retrying."
            );
            return Ok(true);
        }

        if idle_or_desynced {
            if !self.flow.needs_space(command_len) {
                log::info!(
                    "Buffer freed during status poll. Continuing \
                     streaming."
                );
                return Ok(true);
            }
            log::warn!(
                "Deadlock detected during streaming. Attempting G4 \
                 P0.01 recovery."
            );
            self.recover_from_deadlock(false).await;
            if !self.flow.needs_space(command_len) {
                return Ok(true);
            }
            log::error!("Recovery failed: buffer still full.");
            return Ok(false);
        }
        log::info!(
            "Timeout waiting for buffer space (machine not IDLE). \
             This is normal during slow moves. Retrying."
        );
        Ok(true)
    }

    /// Send a realtime status poll and check whether GRBL is idle or
    /// buffer tracking has desynchronized.
    ///
    /// Also tracks liveness in `consecutive_unanswered_polls`:
    /// GRBL answers '?' in every state, so a machine that is merely
    /// busy always answers; liveness is proven by any received
    /// response, not by whether the report could be interpreted.
    ///
    /// When `hold_lock` is true the cmd_lock is taken around the
    /// poll send (the Python default); the stall callback passes
    /// false because its caller already holds the lock.
    async fn poll_and_check_idle(&self, hold_lock: bool) -> bool {
        *self.raw_grbl_status.lock().unwrap() = DeviceStatus::Unknown;
        let responses_before = self.device_response_count();
        let send = async {
            let transport = self.transport_open()?;
            transport.write(b"?").await.map_err(SessionError::from)
        };
        let result = if hold_lock {
            let _guard = self.cmd_lock.lock().await;
            send.await
        } else {
            send.await
        };
        if let Err(err) = result {
            let n = self
                .consecutive_unanswered_polls
                .fetch_add(1, Ordering::SeqCst)
                + 1;
            log::debug!(
                "Failed to send status poll: {err} ({n}/{} unanswered).",
                self.config.unanswered_poll_limit
            );
            return false;
        }
        for _ in 0..self.config.poll_response_attempts {
            tokio::time::sleep(SessionConfig::secs(
                self.config.poll_response_interval,
            ))
            .await;
            if *self.raw_grbl_status.lock().unwrap() != DeviceStatus::Unknown {
                break;
            }
        }
        if self.device_response_count() > responses_before {
            self.consecutive_unanswered_polls.store(0, Ordering::SeqCst);
        } else {
            let n = self
                .consecutive_unanswered_polls
                .fetch_add(1, Ordering::SeqCst)
                + 1;
            log::debug!(
                "No response to status poll ({n}/{} unanswered).",
                self.config.unanswered_poll_limit
            );
        }
        self.is_grbl_idle_or_desynced()
    }

    fn is_grbl_idle_or_desynced(&self) -> bool {
        let status = self.state.lock().unwrap().status;
        if status == DeviceStatus::Idle {
            return true;
        }
        let raw = *self.raw_grbl_status.lock().unwrap();
        if matches!(raw, DeviceStatus::Idle | DeviceStatus::Hold) {
            return true;
        }
        let buf_avail = self.state.lock().unwrap().buffer_available;
        let rx_size = self.flow.rx_buffer_size();
        buf_avail.is_some_and(|v| rx_size > 0 && v >= rx_size as i64)
    }

    fn device_stopped_responding(&self) -> bool {
        self.consecutive_unanswered_polls.load(Ordering::SeqCst)
            >= self.config.unanswered_poll_limit
    }

    fn mark_device_unresponsive(&self) {
        log::error!(
            "No response to {} consecutive status polls. Assuming the \
             device stopped responding.",
            self.consecutive_unanswered_polls.load(Ordering::SeqCst)
        );
        *self.job_exception.lock().unwrap() = Some(SessionError::Device(
            "Device stopped responding during job (no reply to status \
             polls)."
                .to_string(),
        ));
    }

    /// Recover from a detected deadlock: send a `G4 P0.01` dwell;
    /// when its 'ok' arrives the planner buffer is guaranteed empty,
    /// then reset host-side buffer accounting.
    ///
    /// When `hold_lock` is false the caller already holds cmd_lock
    /// (the stall callback inside a gcode send).
    async fn recover_from_deadlock(&self, hold_lock: bool) {
        if !self.transport.is_open() {
            log::warn!("Cannot recover: transport disconnected.");
            return;
        }
        log::info!("Deadlock recovery: sending G4 P0.01 to drain planner.");
        let guard = if hold_lock {
            Some(self.cmd_lock.lock().await)
        } else {
            None
        };
        self.flow.reset_flow_control();
        if let Err(err) = self.send_gcode_simple(b"G4 P0.01\n").await {
            log::warn!("Deadlock recovery failed: {err}");
        }
        drop(guard);
        if self
            .flow
            .wait_pending_empty(Some(Duration::from_secs(30)))
            .await
            .is_ok()
        {
            log::info!("Deadlock recovery: all pending acks received.");
        } else {
            log::warn!(
                "Deadlock recovery: timed out waiting for acks after \
                 G4 P0.01. Resetting host buffers."
            );
        }
        self.flow.reset();
    }

    /// Best-effort gcode send used by safety shutdown and deadlock
    /// recovery (no stall callback).
    async fn send_gcode_simple(&self, data: &[u8]) -> Result<(), SessionError> {
        if self
            .flow
            .wait_for_space(data.len(), Duration::from_secs(10))
            .await
            .is_err()
        {
            return Err(SessionError::BufferStall(
                "timed out waiting for buffer space".to_string(),
            ));
        }
        let text = String::from_utf8_lossy(data).to_string();
        self.flow.track_command(data.len(), None, text.trim_end());
        self.transport.write(data).await?;
        Ok(())
    }

    /// Wait for all pending acks at the end of a job, recovering
    /// from deadlocks.
    async fn drain_pending_acks(&self, timeout: Duration) {
        while !self.flow.pending_queue().is_empty() {
            if self.job_exception.lock().unwrap().is_some()
                || self.state.lock().unwrap().status == DeviceStatus::Alarm
            {
                break;
            }
            if self.is_cancelled() {
                break;
            }
            if self.flow.wait_pending_empty(Some(timeout)).await.is_ok() {
                log::debug!("All 'ok' responses received.");
                break;
            }
            let idle_or_desynced = self.poll_and_check_idle(true).await;
            if self.device_stopped_responding() {
                self.mark_device_unresponsive();
                break;
            }
            if idle_or_desynced {
                if self.flow.pending_queue().is_empty() {
                    log::info!("Pending acks resolved during status poll.");
                    break;
                }
                log::warn!(
                    "Deadlock detected at end of job. Attempting G4 \
                     P0.01 recovery."
                );
                self.recover_from_deadlock(true).await;
            } else {
                log::warn!(
                    "Timeout waiting for acks (machine not IDLE). \
                     Retrying."
                );
            }
        }
    }

    // ---------------------------------------------------------------
    // Cancel / hold
    // ---------------------------------------------------------------

    pub(crate) async fn cancel(
        &self,
        emergency: bool,
    ) -> Result<(), SessionError> {
        log::debug!("Cancel command initiated.");
        let job_was_running = self.job_running.load(Ordering::SeqCst);
        self.is_cancelled.store(true, Ordering::SeqCst);
        self.job_running.store(false, Ordering::SeqCst);

        // Unblock any sender parked waiting for buffer space.
        self.flow.signal_space_available();

        let transport = self.transport_open()?;
        log::info!("Sending Soft Reset (Ctrl-X) to device.");
        transport.write(b"\x18").await?;

        // Hard-abort the streaming sender before resetting the
        // flow-control state, so a sender parked waiting for buffer
        // space can never wake up later and resume a job that was
        // cancelled long before (issue #428).
        self.abort_stream_task().await;

        // Drain the queued interactive commands.
        {
            let queued: Vec<Arc<RequestShared>> = self
                .queued_requests
                .lock()
                .unwrap()
                .drain(..)
                .filter_map(|weak| weak.upgrade())
                .collect();
            for request in queued {
                request.finish();
            }
        }
        log::debug!("Command queue cleared after cancel.");

        // Clear the streaming queue and buffer state.
        self.flow.reset();
        log::debug!("Streaming queue cleared after cancel.");

        self.send_safety_shutdown(emergency).await;

        if job_was_running {
            self.events.job_finished();
        }
        Ok(())
    }

    /// Best-effort transmission of the dialect's tool-off commands
    /// so a cancelled job cannot leave persistent PWM outputs
    /// energized.  After a soft reset the firmware needs a moment to
    /// become ready again, hence the short delay.
    async fn send_safety_shutdown(&self, emergency: bool) {
        let dialect = self.dialect();
        let mut commands = dialect.safety_off_commands.clone();
        if emergency {
            if let Some(cmd) = dialect.emergency_stop.clone() {
                commands.push(cmd);
            }
        }
        if commands.is_empty() {
            return;
        }
        if !self.transport.is_open() {
            return;
        }
        tokio::time::sleep(SessionConfig::secs(
            self.config.safety_shutdown_delay,
        ))
        .await;
        for command in commands {
            if !self.transport.is_open() {
                break;
            }
            let payload = format!("{command}\n");
            if let Err(err) = self.send_gcode_simple(payload.as_bytes()).await {
                log::warn!("Safety command '{command}' failed: {err}");
            }
        }
    }

    pub(crate) async fn set_hold(
        &self,
        hold: bool,
    ) -> Result<(), SessionError> {
        self.is_holding.store(hold, Ordering::SeqCst);
        // Do not un-cancel a job that is still winding down (issue
        // #428).
        if !self.job_running.load(Ordering::SeqCst) {
            let done = self
                .stream_task
                .lock()
                .unwrap()
                .as_ref()
                .is_none_or(|t| t.is_finished());
            if done {
                self.is_cancelled.store(false, Ordering::SeqCst);
            }
        }
        self.send_realtime(if hold { "!" } else { "~" }, false)
            .await?;
        let desired = if hold {
            DeviceStatus::Hold
        } else if self.job_running.load(Ordering::SeqCst) {
            DeviceStatus::Run
        } else {
            DeviceStatus::Idle
        };
        let state_snapshot = {
            let mut state = self.state.lock().unwrap();
            if state.status != desired {
                state.status = desired;
                Some(state.clone())
            } else {
                None
            }
        };
        // Emit outside the lock (see handle_error).
        if let Some(state_snapshot) = state_snapshot {
            self.events.state_changed(&state_snapshot);
        }
        Ok(())
    }

    // ---------------------------------------------------------------
    // Device operations
    // ---------------------------------------------------------------

    pub(crate) async fn home(
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
        // toggling to another WCS and back re-activates it.  Just
        // sending G54 is ignored if GRBL thinks it is already in
        // G54.
        let temp_wcs = if active_wcs == "G54" { "G55" } else { "G54" };
        self.execute_command("G4 P0.01".to_string()).await?;
        self.execute_command(temp_wcs.to_string()).await?;
        self.execute_command(active_wcs.to_string()).await?;
        let state_snapshot = {
            let mut state = self.state.lock().unwrap();
            state.error = None;
            state.clone()
        };
        // Emit outside the lock (see handle_error).
        self.events.state_changed(&state_snapshot);
        Ok(())
    }

    pub(crate) async fn move_to(
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

    pub(crate) async fn jog(
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

    pub(crate) async fn select_tool(
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

    pub(crate) async fn set_power(
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

    pub(crate) async fn set_focus_power(
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
            _ => {
                self.wait_for_idle().await;
                dialect.laser_off.clone()
            }
        };
        self.execute_command(cmd).await?;
        Ok(())
    }

    async fn wait_for_idle(&self) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while self.state.lock().unwrap().status == DeviceStatus::Jog {
            if tokio::time::Instant::now() > deadline {
                log::warn!(
                    "Timed out waiting for JOG to finish before \
                     sending laser-off command."
                );
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub(crate) async fn read_settings(
        &self,
    ) -> Result<Vec<(String, String)>, SessionError> {
        let lines = self.execute_interactive_command("$$".to_string()).await?;
        self.report_in_inches
            .store(parser::is_report_in_inches(&lines), Ordering::SeqCst);
        Ok(parser::parse_setting_pairs(&lines))
    }

    pub(crate) async fn write_setting(
        &self,
        key: &str,
        value: &str,
    ) -> Result<(), SessionError> {
        self.execute_command(format!("${key}={value}")).await?;
        Ok(())
    }

    pub(crate) async fn set_wcs_offset(
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

    pub(crate) async fn read_wcs_offsets(
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

    pub(crate) async fn read_parser_state(
        &self,
    ) -> Result<Option<String>, SessionError> {
        let lines = self.execute_interactive_command("$G".to_string()).await?;
        Ok(parser::parse_grbl_parser_state(&lines))
    }

    pub(crate) async fn run_probe_cycle(
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

    pub(crate) async fn detect_unit_system(
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
