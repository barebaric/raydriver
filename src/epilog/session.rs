//! The Epilog LPD session: probe reachability and upload print jobs.
//!
//! See the module docs for the wire exchange. The session opens one
//! connection per operation and never retries a partial job.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task::JoinHandle;

use crate::core::connector::{dial, DynStream};
use crate::core::session::{
    unsupported, CancelMode, Session, SessionError, SessionEvents,
};
use crate::core::types::{DeviceState, DeviceStatus, TransportStatus};

/// Default LPD port of a printer.
pub const DEFAULT_PORT: u16 = 515;

/// Default deadline for connecting, writing and reading one ack.
const DEFAULT_TIMEOUT: f64 = 30.0;

/// Write chunk size; the control transfer must not stall the machine.
const WRITE_CHUNK: usize = 64 * 1024;

/// Connection settings for an Epilog machine.
#[derive(Clone)]
pub struct EpilogConfig {
    pub host: String,
    pub port: u16,
    /// Per-stage connect/write/ack deadline in seconds.
    pub timeout: f64,
}

impl EpilogConfig {
    pub fn new(host: &str) -> Self {
        Self {
            host: host.to_string(),
            port: DEFAULT_PORT,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    fn deadline(&self) -> Duration {
        Duration::from_secs_f64(self.timeout.max(0.001))
    }
}

/// A submitted job the caller can still cancel while it uploads.
struct ActiveUpload {
    task: JoinHandle<Result<(), SessionError>>,
}

/// The Epilog LPD session core.
pub struct EpilogSession {
    config: EpilogConfig,
    events: Arc<dyn SessionEvents>,
    state: Mutex<DeviceState>,
    connected: AtomicBool,
    uploading: AtomicBool,
    /// Monotonic id so a late upload result cannot clear a newer one.
    upload_seq: AtomicU64,
    upload: Mutex<Option<ActiveUpload>>,
}

impl EpilogSession {
    pub fn new(config: EpilogConfig, events: Arc<dyn SessionEvents>) -> Self {
        Self {
            config,
            events,
            state: Mutex::new(DeviceState::default()),
            connected: AtomicBool::new(false),
            uploading: AtomicBool::new(false),
            upload_seq: AtomicU64::new(0),
            upload: Mutex::new(None),
        }
    }

    fn set_status(&self, status: DeviceStatus) {
        let mut state = self.state.lock().unwrap();
        state.status = status;
        let snapshot = state.clone();
        drop(state);
        self.events.state_changed(&snapshot);
    }

    fn set_error(&self, message: Option<String>) {
        let mut state = self.state.lock().unwrap();
        state.error = message.as_ref().map(|message| {
            crate::core::types::DeviceError::new(1, "EPILOG", message)
        });
        let snapshot = state.clone();
        drop(state);
        self.events.state_changed(&snapshot);
    }

    fn connection(&self, status: TransportStatus, message: Option<&str>) {
        self.events.connection_status(status, message);
    }

    /// Open a TCP connection, send nothing, close it again.
    ///
    /// The LPD port accepts a connection only when the printer is
    /// reachable, so this is the whole reachability check; the
    /// machine exposes no status query.
    pub async fn probe(&self) -> Result<(), SessionError> {
        let deadline = self.config.deadline();
        let mut stream = tokio::time::timeout(
            deadline,
            dial(&self.config.host, self.config.port),
        )
        .await
        .map_err(|_| {
            SessionError::Timeout(format!(
                "connecting to {}:{} timed out",
                self.config.host, self.config.port
            ))
        })?
        .map_err(|e| SessionError::Connection(e.to_string()))?;
        let _ = stream.shutdown().await;
        Ok(())
    }

    /// Upload a complete print job and wait until the machine has
    /// accepted it.
    ///
    /// *payload* is the encoder's byte stream; *title* names the job
    /// in the machine's job list.  The transfer runs in a tracked
    /// task so `cancel` (or the asyncio future's cancellation) can
    /// abort it mid-flight.
    pub async fn upload(
        self: Arc<Self>,
        payload: Vec<u8>,
        title: String,
    ) -> Result<(), SessionError> {
        if payload.is_empty() {
            return Err(SessionError::Device(
                "the Epilog job is empty".to_string(),
            ));
        }
        if self.uploading.swap(true, Ordering::SeqCst) {
            return Err(SessionError::Device(
                "an Epilog upload is already in progress".to_string(),
            ));
        }
        let seq = self.upload_seq.fetch_add(1, Ordering::SeqCst) + 1;
        self.set_status(DeviceStatus::Queue);
        self.events
            .command_status(TransportStatus::Connecting, None);

        let session = self.clone();
        let task =
            tokio::spawn(async move {
                let result = session.transfer(&payload, &title).await.map_err(
                    |e| match e {
                        TransferError::Connection(message) => {
                            SessionError::Connection(message)
                        }
                        TransferError::Timeout(message) => {
                            SessionError::Timeout(message)
                        }
                        TransferError::Rejected(message) => {
                            SessionError::Device(message)
                        }
                    },
                );
                session.finish_upload(seq, &result);
                result
            });
        self.track_upload(task, seq);

        let Some(task) = self.upload_task() else {
            return Err(SessionError::Device(
                "Epilog upload was cancelled".to_string(),
            ));
        };
        match task.await {
            Ok(result) => result,
            Err(join) if join.is_cancelled() => Err(SessionError::Device(
                "Epilog upload was cancelled".to_string(),
            )),
            Err(join) => Err(SessionError::Other(join.to_string())),
        }
    }

    /// Publishes the terminal state of an upload unless it has been
    /// superseded by a newer one.
    fn finish_upload(&self, seq: u64, result: &Result<(), SessionError>) {
        if seq != self.upload_seq.load(Ordering::SeqCst) {
            return;
        }
        self.uploading.store(false, Ordering::SeqCst);
        self.upload.lock().unwrap().take();
        match result {
            Ok(()) => {
                self.set_error(None);
                self.set_status(DeviceStatus::Idle);
                self.events.command_status(TransportStatus::Idle, None);
                self.events.job_finished();
            }
            Err(error) => {
                let message = error.to_string();
                self.set_error(Some(message.clone()));
                self.set_status(DeviceStatus::Unknown);
                self.events
                    .command_status(TransportStatus::Error, Some(&message));
                self.events.job_error(&message);
            }
        }
    }

    /// Perform the LPD exchange on one fresh connection.
    async fn transfer(
        &self,
        payload: &[u8],
        title: &str,
    ) -> Result<(), TransferError> {
        let deadline = self.config.deadline();
        let mut stream = tokio::time::timeout(
            deadline,
            dial(&self.config.host, self.config.port),
        )
        .await
        .map_err(|_| {
            TransferError::Timeout("connecting to the Epilog timed out".into())
        })?
        .map_err(|e| TransferError::Connection(e.to_string()))?;

        let exchange =
            self.exchange(&mut stream, payload, title, deadline).await;
        let _ = stream.shutdown().await;
        exchange
    }

    async fn exchange(
        &self,
        stream: &mut DynStream,
        payload: &[u8],
        title: &str,
        deadline: Duration,
    ) -> Result<(), TransferError> {
        let token = upload_token();
        let name = format!("dfA{token}rayforge");
        let control_name = format!("cfA{token}rayforge");
        let control = control_file(title, &name);

        self.stage(stream, b"\x02\n", "receive a job", deadline)
            .await?;

        let header = format!("\x02{} {control_name}\n", control.len());
        self.stage(stream, header.as_bytes(), "the control header", deadline)
            .await?;

        let mut control_block = control;
        control_block.push(0);
        self.stage(stream, &control_block, "the control file", deadline)
            .await?;

        let data_header = format!("\x03{} {name}\n", payload.len());
        self.stage(stream, data_header.as_bytes(), "the data header", deadline)
            .await?;

        self.write_all(stream, payload, deadline).await?;
        self.read_ack(stream, "the job data", deadline).await?;
        Ok(())
    }

    async fn stage(
        &self,
        stream: &mut DynStream,
        bytes: &[u8],
        label: &str,
        deadline: Duration,
    ) -> Result<(), TransferError> {
        self.write_all(stream, bytes, deadline).await?;
        self.read_ack(stream, label, deadline).await
    }

    async fn write_all(
        &self,
        stream: &mut DynStream,
        bytes: &[u8],
        deadline: Duration,
    ) -> Result<(), TransferError> {
        for chunk in bytes.chunks(WRITE_CHUNK) {
            tokio::time::timeout(deadline, stream.write_all(chunk))
                .await
                .map_err(|_| {
                    TransferError::Timeout(
                        "writing the Epilog job timed out".into(),
                    )
                })?
                .map_err(|e| TransferError::Connection(e.to_string()))?;
        }
        tokio::time::timeout(deadline, stream.flush())
            .await
            .map_err(|_| {
                TransferError::Timeout(
                    "flushing the Epilog job timed out".into(),
                )
            })?
            .map_err(|e| TransferError::Connection(e.to_string()))?;
        Ok(())
    }

    async fn read_ack(
        &self,
        stream: &mut DynStream,
        label: &str,
        deadline: Duration,
    ) -> Result<(), TransferError> {
        let mut ack = [0u8; 1];
        tokio::time::timeout(deadline, stream.read_exact(&mut ack))
            .await
            .map_err(|_| {
                TransferError::Timeout(format!(
                    "Epilog timed out acknowledging {label}"
                ))
            })?
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    TransferError::Connection(format!(
                        "Epilog disconnected during {label}"
                    ))
                } else {
                    TransferError::Connection(e.to_string())
                }
            })?;
        match ack[0] {
            0 => Ok(()),
            code => Err(TransferError::Rejected(format!(
                "Epilog rejected {label} (code {code})"
            ))),
        }
    }

    /// Stop the in-flight upload, if any.
    async fn cancel_upload(&self) -> bool {
        let active = self.upload.lock().unwrap().take();
        let Some(active) = active else {
            return false;
        };
        self.upload_seq.fetch_add(1, Ordering::SeqCst);
        active.task.abort();
        let _ = active.task.await;
        self.uploading.store(false, Ordering::SeqCst);
        self.events.job_error(
            "Epilog upload interrupted; inspect the job list at the machine",
        );
        true
    }
}

/// Failure categories of the LPD exchange.
enum TransferError {
    Connection(String),
    Timeout(String),
    Rejected(String),
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connection(m) | Self::Timeout(m) | Self::Rejected(m) => {
                write!(f, "{m}")
            }
        }
    }
}

/// The control file describing one job.
///
/// `H` is the submitting host, `P` the submitting user, `J` the job
/// name, and the `l`/`U` lines name the data file.
fn control_file(title: &str, data_name: &str) -> Vec<u8> {
    let job = sanitize_title(title);
    format!("Hrayforge\nPrayforge\nJ{job}\nl{data_name}\nU{data_name}\n")
        .into_bytes()
}

/// Keep the job name printable and free of LPD control characters.
fn sanitize_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len().min(64));
    for ch in title.chars() {
        if out.len() >= 64 {
            break;
        }
        if ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '.' | '_' | '-') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        "Rayforge".to_string()
    } else {
        trimmed.to_string()
    }
}

/// A three-digit suffix that keeps queued jobs distinguishable.
fn upload_token() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_micros())
        .unwrap_or(0);
    format!("{:03}", nanos % 1000)
}

#[async_trait]
impl Session for EpilogSession {
    async fn connect(self: Arc<Self>) -> Result<(), SessionError> {
        self.connection(TransportStatus::Connecting, None);
        match self.probe().await {
            Ok(()) => {
                self.connected.store(true, Ordering::SeqCst);
                self.set_status(DeviceStatus::Idle);
                self.connection(
                    TransportStatus::Connected,
                    Some("Epilog reachable; machine status is unavailable"),
                );
                Ok(())
            }
            Err(error) => {
                self.connected.store(false, Ordering::SeqCst);
                self.set_status(DeviceStatus::Unknown);
                self.connection(
                    TransportStatus::Error,
                    Some(&error.to_string()),
                );
                Err(error)
            }
        }
    }

    async fn cleanup(&self) {
        self.connected.store(false, Ordering::SeqCst);
        self.cancel_upload().await;
        self.connection(TransportStatus::Disconnected, None);
    }

    fn state(&self) -> DeviceState {
        self.state.lock().unwrap().clone()
    }

    fn is_job_running(&self) -> bool {
        self.uploading.load(Ordering::SeqCst)
    }

    fn resource_uri(&self) -> Option<String> {
        Some(format!("tcp://{}:{}", self.config.host, self.config.port))
    }

    fn abort_background_tasks_now(&self) {
        if let Some(active) = self.upload.lock().unwrap().take() {
            active.task.abort();
        }
        self.uploading.store(false, Ordering::SeqCst);
    }

    async fn cancel(&self, _mode: CancelMode) -> Result<(), SessionError> {
        self.cancel_upload().await;
        Ok(())
    }

    async fn run(
        self: Arc<Self>,
        _gcode: &str,
        _op_map: &std::collections::HashMap<usize, i64>,
        _op_estimates: &[f64],
    ) -> Result<(), SessionError> {
        Err(unsupported("run"))
    }

    async fn read_wcs_offsets(
        &self,
    ) -> Result<std::collections::HashMap<String, (f64, f64, f64)>, SessionError>
    {
        Ok([("MACHINE".to_string(), (0.0, 0.0, 0.0))]
            .into_iter()
            .collect())
    }

    async fn read_parser_state(&self) -> Result<Option<String>, SessionError> {
        Ok(Some("MACHINE".to_string()))
    }

    async fn select_tool(&self, _tool_number: i64) -> Result<(), SessionError> {
        Ok(())
    }
}

impl EpilogSession {
    /// Store the upload task so `cancel` can abort it.
    fn track_upload(
        &self,
        task: JoinHandle<Result<(), SessionError>>,
        _seq: u64,
    ) {
        *self.upload.lock().unwrap() = Some(ActiveUpload { task });
    }

    /// Take the tracked upload task out so the caller can await it.
    fn upload_task(&self) -> Option<JoinHandle<Result<(), SessionError>>> {
        self.upload.lock().unwrap().take().map(|active| active.task)
    }
}
