//! The `GrblSession` PyO3 wrapper: thin async methods delegating to
//! the Rust session core.

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::exceptions::{
    PyConnectionError, PyNotImplementedError, PyRuntimeError, PyTimeoutError,
    PyValueError,
};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::core::dialect::Dialect;
use crate::core::session::{CancelMode, Session, SessionError};
use crate::core::transport::TransportKind;

use crate::esp3d::transport::{Esp3dConfig, Esp3dTransport, ProtocolVariant};
use crate::grbl::session::{GrblSession, SessionConfig};

use crate::python::core::mock::MockTransport;
use crate::python::core::types::DeviceState;
use crate::python::events::PyEvents;
use crate::python::grbl::probe::GrblDeviceInfo;
use crate::python::pyfuture::{spawn_future, PyAwait};

impl From<SessionError> for PyErr {
    fn from(err: SessionError) -> Self {
        match err {
            SessionError::Connection(msg) => PyConnectionError::new_err(msg),
            SessionError::Timeout(message) => PyTimeoutError::new_err(message),
            SessionError::Device(msg) => PyRuntimeError::new_err(msg),
            SessionError::BufferStall(msg) => {
                PyConnectionError::new_err(format!("buffer stall: {msg}"))
            }
            SessionError::Unsupported(msg) => {
                PyNotImplementedError::new_err(msg)
            }
            SessionError::Other(msg) => PyRuntimeError::new_err(msg),
        }
    }
}

pub(crate) fn get_bool(
    dict: &Bound<'_, PyDict>,
    key: &str,
) -> PyResult<Option<bool>> {
    match dict.get_item(key)? {
        Some(v) => Ok(Some(v.extract::<bool>()?)),
        None => Ok(None),
    }
}

pub(crate) fn get_f64(
    dict: &Bound<'_, PyDict>,
    key: &str,
) -> PyResult<Option<f64>> {
    match dict.get_item(key)? {
        Some(v) => {
            if let Ok(b) = v.extract::<bool>() {
                return Ok(Some(if b { 1.0 } else { 0.0 }));
            }
            Ok(Some(v.extract::<f64>()?))
        }
        None => Ok(None),
    }
}

pub(crate) fn get_i64(
    dict: &Bound<'_, PyDict>,
    key: &str,
) -> PyResult<Option<i64>> {
    match dict.get_item(key)? {
        Some(v) => Ok(Some(v.extract::<i64>()?)),
        None => Ok(None),
    }
}

pub(crate) fn get_str(
    dict: &Bound<'_, PyDict>,
    key: &str,
) -> PyResult<Option<String>> {
    match dict.get_item(key)? {
        Some(v) => Ok(Some(v.extract::<String>()?)),
        None => Ok(None),
    }
}

pub(crate) fn build_config(
    kind: TransportKind,
    dict: Option<&Bound<'_, PyDict>>,
) -> PyResult<SessionConfig> {
    let mut config = SessionConfig::new(kind);
    let Some(dict) = dict else {
        return Ok(config);
    };
    if let Some(v) = get_bool(dict, "poll_status_while_running")? {
        config.poll_status_while_running = v;
    }
    if let Some(v) = get_bool(dict, "deadlock_detection")? {
        config.deadlock_detection = v;
    }
    if let Some(v) = get_i64(dict, "rx_buffer_size_override")? {
        config.rx_buffer_size_override = v;
    }
    if let Some(v) = get_i64(dict, "cached_rx_buffer_size")? {
        config.cached_rx_buffer_size = if v > 0 { Some(v) } else { None };
    }
    if let Some(v) = get_f64(dict, "stall_timeout_min")? {
        config.stall_timeout_min = v;
    }
    if let Some(v) = get_f64(dict, "stall_timeout_max")? {
        config.stall_timeout_max = v;
    }
    if let Some(v) = get_f64(dict, "stall_timeout_safety_factor")? {
        config.stall_timeout_safety_factor = v;
    }
    if let Some(v) = get_f64(dict, "stall_timeout_default")? {
        config.stall_timeout_default = v;
    }
    if let Some(v) = get_f64(dict, "safety_shutdown_delay")? {
        config.safety_shutdown_delay = v;
    }
    if let Some(v) = get_i64(dict, "unanswered_poll_limit")? {
        config.unanswered_poll_limit = v.max(0) as u32;
    }
    if let Some(v) = get_i64(dict, "poll_response_attempts")? {
        config.poll_response_attempts = v.max(0) as u32;
    }
    if let Some(v) = get_f64(dict, "poll_response_interval")? {
        config.poll_response_interval = v;
    }
    if let Some(v) = get_f64(dict, "handshake_timeout")? {
        config.handshake_timeout = v;
    }
    if let Some(v) = get_f64(dict, "handshake_poll_interval")? {
        config.handshake_poll_interval = v;
    }
    if let Some(v) = get_f64(dict, "status_poll_interval")? {
        config.status_poll_interval = v;
    }
    if let Some(v) = get_f64(dict, "reconnect_delay")? {
        config.reconnect_delay = v;
    }
    if let Some(v) = get_f64(dict, "command_timeout")? {
        config.command_timeout = v;
    }
    if let Some(v) = get_f64(dict, "homing_timeout")? {
        config.homing_timeout = v;
    }
    Ok(config)
}

pub(crate) fn transport_kind_from_config(
    dict: Option<&Bound<'_, PyDict>>,
) -> PyResult<TransportKind> {
    let Some(dict) = dict else {
        return Err(PyRuntimeError::new_err(
            "config with a 'port' (serial), 'host' (telnet) or 'address' (Bluetooth) is required",
        ));
    };
    if let Some(host) = get_str(dict, "host")? {
        if dict.get_item("http_port")?.is_some()
            || dict.get_item("ws_port")?.is_some()
        {
            let mut esp3d = Esp3dConfig::new(&host);
            esp3d.http_port = get_i64(dict, "http_port")?
                .or(get_i64(dict, "port")?)
                .unwrap_or(80) as u16;
            esp3d.ws_port = get_i64(dict, "ws_port")?.unwrap_or(81) as u16;
            esp3d.protocol = ProtocolVariant::from_name(
                get_str(dict, "protocol")?.as_deref().unwrap_or_default(),
            );
            if let Some(timeout) = get_f64(dict, "request_timeout")? {
                esp3d.request_timeout = timeout;
            }
            if let Some(timeout) = get_f64(dict, "connect_timeout")? {
                esp3d.connect_timeout = timeout;
            }
            return Ok(TransportKind::Custom(Esp3dTransport::new(esp3d)));
        }
        let port = get_i64(dict, "tcp_port")?.unwrap_or(23) as u16;
        return Ok(TransportKind::Telnet { host, port });
    }
    if let Some(address) = get_str(dict, "address")? {
        let channel = get_i64(dict, "channel")?.unwrap_or(1);
        if !(1..=30).contains(&channel) {
            return Err(PyValueError::new_err(
                "Bluetooth channel must be between 1 and 30",
            ));
        }
        let address =
            crate::core::transport::bluetooth::normalize_address(&address)
                .map_err(PyValueError::new_err)?;
        return Ok(TransportKind::Bluetooth {
            address,
            channel: channel as u8,
        });
    }
    let port = get_str(dict, "port")?.unwrap_or_default();
    let baudrate = get_i64(dict, "baudrate")?.unwrap_or(115200) as u32;
    Ok(TransportKind::Serial { port, baudrate })
}

pub(crate) fn parse_dialect(
    dict: Option<&Bound<'_, PyDict>>,
) -> PyResult<Dialect> {
    let mut safety_off_commands = None;
    let mut map: HashMap<String, String> = HashMap::new();
    if let Some(dict) = dict {
        for (key, value) in dict.iter() {
            let key: String = key.extract()?;
            if key == "safety_off_commands" {
                safety_off_commands = Some(value.extract::<Vec<String>>()?);
                continue;
            }
            if let Ok(text) = value.extract::<String>() {
                map.insert(key, text);
            }
        }
    }
    let mut dialect = Dialect::from_map(&map);
    if let Some(commands) = safety_off_commands {
        dialect.safety_off_commands = commands;
    }
    Ok(dialect)
}

/// A GRBL device session: connection lifecycle, streaming and
/// interactive commands, entirely in Rust.
///
/// All async methods return asyncio futures; a typed `SessionEvent`
/// is delivered to `event_callback(event)` on the event loop thread.
#[gen_stub_pyclass]
#[pyclass(name = "GrblSession", module = "raydriver.grbl")]
pub struct PyGrblSession {
    core: Arc<GrblSession>,
    events: Arc<PyEvents>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyGrblSession {
    #[new]
    #[pyo3(signature = (config, dialect=None, event_callback=None))]
    fn new(
        #[gen_stub(override_type(
            type_repr = "typing.Optional[typing.Union[dict, config.GrblSerialConfig, config.GrblTelnetConfig, config.GrblBluetoothConfig, config.Esp3dConfig]]",
            imports = ("typing", "raydriver.config")
        ))]
        config: Option<Bound<'_, PyAny>>,

        dialect: Option<Bound<'_, PyDict>>,
        #[gen_stub(override_type(
            type_repr = "typing.Optional[collections.abc.Callable[[core.SessionEvent], builtins.object]]",
            imports = ("builtins", "collections.abc", "typing", "raydriver.core")
        ))]
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let config = crate::python::config::config_dict(config.as_ref())?;
        let kind = transport_kind_from_config(config.as_ref())?;
        let session_config = build_config(kind, config.as_ref())?;
        Self::build(session_config, dialect, event_callback)
    }

    /// Build a session around a [`MockTransport`] (for tests).
    #[staticmethod]
    #[pyo3(signature = (transport, config=None, dialect=None, event_callback=None))]
    fn with_transport(
        transport: PyRef<'_, MockTransport>,
        #[gen_stub(override_type(
            type_repr = "typing.Optional[typing.Union[dict, config.GrblSerialConfig, config.GrblTelnetConfig, config.GrblBluetoothConfig, config.Esp3dConfig]]",
            imports = ("typing", "raydriver.config")
        ))]
        config: Option<Bound<'_, PyAny>>,

        dialect: Option<Bound<'_, PyDict>>,
        #[gen_stub(override_type(
            type_repr = "typing.Optional[collections.abc.Callable[[core.SessionEvent], builtins.object]]",
            imports = ("builtins", "collections.abc", "typing", "raydriver.core")
        ))]
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let config = crate::python::config::config_dict(config.as_ref())?;
        let kind = TransportKind::Mock(transport.inner.clone());
        let session_config = build_config(kind, config.as_ref())?;
        Self::build(session_config, dialect, event_callback)
    }

    /// Open the connection and start the background tasks.
    /// Run the device census: query `$I` and `$$`, and return the
    /// structured result as a dict.
    fn probe_device<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<PyAwait<'py, GrblDeviceInfo>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            let build_info = core.execute_interactive_command("$I").await?;
            let settings_lines = core.execute_interactive_command("$$").await?;
            Ok(GrblDeviceInfo {
                inner: crate::grbl::probe::describe(
                    &build_info,
                    &settings_lines,
                ),
            })
        })
    }
    /// Current snapshot of the device state.
    #[getter]
    fn state(&self) -> DeviceState {
        DeviceState::new(self.core.state())
    }

    #[getter]
    fn buffer_count(&self) -> usize {
        self.core.buffer_count()
    }

    #[getter]
    fn rx_buffer_size(&self) -> usize {
        self.core.rx_buffer_size()
    }

    /// Commands awaiting their 'ok' acknowledgement.
    #[getter]
    fn pending_commands(&self) -> Vec<String> {
        self.core.pending_commands()
    }

    #[getter]
    fn job_running(&self) -> bool {
        self.core.is_job_running()
    }

    #[getter]
    fn resource_uri(&self) -> Option<String> {
        self.core.resource_uri()
    }

    fn __repr__(&self) -> String {
        format!(
            "GrblSession(resource_uri={:?}, job_running={})",
            self.resource_uri(),
            self.job_running()
        )
    }
}

impl PyGrblSession {
    fn build(
        config: SessionConfig,
        dialect: Option<Bound<'_, PyDict>>,
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let dialect = parse_dialect(dialect.as_ref())?;
        let events = Arc::new(PyEvents::new(event_callback));
        let core = GrblSession::new(config, dialect, events.clone());
        Ok(Self { core, events })
    }
}

impl Drop for PyGrblSession {
    fn drop(&mut self) {
        // Stop the background tasks even when the session is dropped
        // without disconnect(); otherwise they orphan on the shared
        // runtime and marshal events onto a possibly-closed loop.
        self.events.close();
        self.core.abort_background_tasks_now();
    }
}

use crate::python::session_api::uniform_session_api;

uniform_session_api!(PyGrblSession);
