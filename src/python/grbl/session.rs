//! The `GrblSession` PyO3 wrapper: thin async methods delegating to
//! the Rust session core.

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::exceptions::{
    PyConnectionError, PyNotImplementedError, PyRuntimeError, PyTimeoutError,
};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::core::dialect::Dialect;
use crate::core::session::SessionError;
use crate::core::transport::TransportKind;
use crate::core::types::{UnitSystem, WcsOffsets};
use crate::esp3d::transport::{Esp3dConfig, Esp3dTransport, ProtocolVariant};
use crate::grbl::session::{SessionConfig, SessionCore};

use crate::python::core::mock::MockTransport;
use crate::python::core::types::DeviceState;
use crate::python::events::PyEvents;
use crate::python::grbl::probe::GrblDeviceInfo;
use crate::python::pyfuture::{spawn_future, PyAwait};

impl From<SessionError> for PyErr {
    fn from(err: SessionError) -> Self {
        match err {
            SessionError::Connection(msg) => PyConnectionError::new_err(msg),
            SessionError::Timeout => PyTimeoutError::new_err(err.to_string()),
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
    Ok(config)
}

pub(crate) fn transport_kind_from_config(
    dict: Option<&Bound<'_, PyDict>>,
) -> PyResult<TransportKind> {
    let Some(dict) = dict else {
        return Err(PyRuntimeError::new_err(
            "config with a 'port' (serial) or 'host' (telnet) is required",
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
#[pyclass(module = "raydriver.grbl")]
pub struct GrblSession {
    core: Arc<SessionCore>,
    events: Arc<PyEvents>,
}

#[gen_stub_pymethods]
#[pymethods]
impl GrblSession {
    #[new]
    #[pyo3(signature = (config, dialect=None, event_callback=None))]
    fn new(
        #[gen_stub(override_type(
            type_repr = "typing.Optional[typing.Union[dict, config.GrblSerialConfig, config.Esp3dConfig]]",
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
            type_repr = "typing.Optional[typing.Union[dict, config.GrblSerialConfig, config.Esp3dConfig]]",
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
            let build_info =
                core.execute_interactive_command("$I".to_string()).await?;
            let settings_lines =
                core.execute_interactive_command("$$".to_string()).await?;
            Ok(GrblDeviceInfo {
                inner: crate::grbl::probe::describe(
                    &build_info,
                    &settings_lines,
                ),
            })
        })
    }

    fn connect<'py>(&self, py: Python<'py>) -> PyResult<PyAwait<'py, ()>> {
        self.events.capture_loop(py)?;
        let core = self.core.clone();
        spawn_future(py, async move {
            core.connect().await;
            Ok(())
        })
    }

    /// Stop all background tasks and close the transport.
    fn disconnect<'py>(&self, py: Python<'py>) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        let events = self.events.clone();
        spawn_future(py, async move {
            core.cleanup().await;
            events.quiesce();
            Ok(())
        })
    }

    /// Send a command and synchronously await its full response,
    /// blocking any other command from interleaving.
    fn execute_interactive_command<'py>(
        &self,
        py: Python<'py>,
        command: String,
    ) -> PyResult<PyAwait<'py, Vec<String>>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            Ok(core.execute_interactive_command(command).await?)
        })
    }

    /// Queue a command for asynchronous processing and await its
    /// response.
    fn execute_command<'py>(
        &self,
        py: Python<'py>,
        command: String,
    ) -> PyResult<PyAwait<'py, Vec<String>>> {
        let core = self.core.clone();
        spawn_future(
            py,
            async move { Ok(core.execute_command(command).await?) },
        )
    }

    /// Stream a G-code program using the character-counting
    /// protocol.
    ///
    /// `op_line_map` maps G-code line indices to op indices for
    /// progress reporting; `op_estimates` holds per-op estimated
    /// durations (seconds) used to scale per-line stall timeouts.
    #[pyo3(signature = (gcode, op_line_map=None, op_estimates=None, progress_callback=None))]
    fn run<'py>(
        &self,
        py: Python<'py>,
        gcode: String,
        op_line_map: Option<HashMap<usize, i64>>,
        op_estimates: Option<Vec<f64>>,
        #[gen_stub(override_type(
            type_repr = "typing.Optional[collections.abc.Callable[[builtins.int], builtins.object]]",
            imports = ("typing", "collections.abc")
        ))]
        progress_callback: Option<Py<PyAny>>,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        let events = self.events.clone();
        let op_map = op_line_map.unwrap_or_default();
        let estimates = op_estimates.unwrap_or_default();
        spawn_future(py, async move {
            // The callback stays installed after the run returns:
            // the final command_done events are marshaled from the
            // reader task and may be delivered moments later (the
            // Python driver keeps _on_command_done until its stream
            // finally-block too). The next run replaces it.
            events.set_progress_callback(progress_callback);
            core.run(&gcode, op_map, estimates).await;
            Ok(())
        })
    }

    /// Execute a raw G-code string; GRBL realtime commands (?, ~, !)
    /// are sent via the control path instead of the stream.
    fn run_raw<'py>(
        &self,
        py: Python<'py>,
        machine_code: String,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.run_raw(&machine_code).await;
            Ok(())
        })
    }

    /// Pause (feed hold) or resume the device.
    #[pyo3(signature = (hold=true))]
    fn set_hold<'py>(
        &self,
        py: Python<'py>,
        hold: bool,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.set_hold(hold).await?;
            Ok(())
        })
    }

    /// Abort the running job: soft reset, queue cleanup and safety
    /// shutdown.
    #[pyo3(signature = (emergency=false))]
    fn cancel<'py>(
        &self,
        py: Python<'py>,
        emergency: bool,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.cancel(emergency).await?;
            Ok(())
        })
    }

    /// Home all axes (`axes` is None) or the named axes (e.g.
    /// `["X"]`), then re-activate the active WCS via the toggle
    /// workaround.
    #[pyo3(signature = (axes=None, active_wcs = String::from("G54")))]
    fn home<'py>(
        &self,
        py: Python<'py>,
        axes: Option<Vec<String>>,
        active_wcs: String,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.home(axes, &active_wcs).await?;
            Ok(())
        })
    }

    /// Move to an absolute position (mm) at `speed` (mm/min).
    fn move_to<'py>(
        &self,
        py: Python<'py>,
        speed: f64,
        x: f64,
        y: f64,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.move_to(speed, x, y).await?;
            Ok(())
        })
    }

    /// Jog by relative deltas (mm) at `speed` (mm/min).
    fn jog<'py>(
        &self,
        py: Python<'py>,
        speed: f64,
        deltas: Vec<(String, f64)>,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.jog(speed, &deltas).await?;
            Ok(())
        })
    }

    fn select_tool<'py>(
        &self,
        py: Python<'py>,
        tool_number: i64,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.select_tool(tool_number).await?;
            Ok(())
        })
    }

    /// Set laser power.  `power` is the absolute S value; `None`
    /// (or zero) turns the laser off.
    #[pyo3(signature = (power=None))]
    fn set_power<'py>(
        &self,
        py: Python<'py>,
        power: Option<f64>,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.set_power(power).await?;
            Ok(())
        })
    }

    /// Set laser power for focus mode; `None` waits for pending jogs
    /// to finish and turns the laser off.
    #[pyo3(signature = (power=None))]
    fn set_focus_power<'py>(
        &self,
        py: Python<'py>,
        power: Option<f64>,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.set_focus_power(power).await?;
            Ok(())
        })
    }

    /// Read `$$` into `(key, raw_value)` pairs.
    fn read_settings<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<PyAwait<'py, Vec<(String, String)>>> {
        let core = self.core.clone();
        spawn_future(py, async move { Ok(core.read_settings().await?) })
    }

    fn write_setting<'py>(
        &self,
        py: Python<'py>,
        key: String,
        value: String,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.write_setting(&key, &value).await?;
            Ok(())
        })
    }

    fn set_wcs_offset<'py>(
        &self,
        py: Python<'py>,
        wcs_slot: String,
        x: f64,
        y: f64,
        z: Option<f64>,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.set_wcs_offset(&wcs_slot, x, y, z).await?;
            Ok(())
        })
    }

    /// Read `$#` WCS offsets, converted to mm when the device
    /// reports inches.
    fn read_wcs_offsets<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<PyAwait<'py, WcsOffsets>> {
        let core = self.core.clone();
        spawn_future(py, async move { Ok(core.read_wcs_offsets().await?) })
    }

    /// Read the `$G` parser state to determine the active WCS.
    fn read_parser_state<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<PyAwait<'py, Option<String>>> {
        let core = self.core.clone();
        spawn_future(py, async move { Ok(core.read_parser_state().await?) })
    }

    /// Run a probe cycle (G38.2); returns the trigger position in mm
    /// or None on failure.
    fn run_probe_cycle<'py>(
        &self,
        py: Python<'py>,
        axis_letter: String,
        max_travel: f64,
        feed_rate: f64,
    ) -> PyResult<PyAwait<'py, Option<(f64, f64, f64)>>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            Ok(core
                .run_probe_cycle(&axis_letter, max_travel, feed_rate)
                .await?)
        })
    }

    /// Infer the device's unit system ("metric"/"imperial") from the
    /// `$13` setting.
    fn detect_unit_system<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<PyAwait<'py, Option<String>>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            let unit = core.detect_unit_system().await?;
            Ok(unit.map(|u| match u {
                UnitSystem::Metric => "metric".to_string(),
                UnitSystem::Imperial => "imperial".to_string(),
            }))
        })
    }

    /// Swap the dialect command templates at runtime.
    fn update_dialect(&self, dialect: Bound<'_, PyDict>) -> PyResult<()> {
        self.core.update_dialect(parse_dialect(Some(&dialect))?);
        Ok(())
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

impl GrblSession {
    fn build(
        config: SessionConfig,
        dialect: Option<Bound<'_, PyDict>>,
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let dialect = parse_dialect(dialect.as_ref())?;
        let events = Arc::new(PyEvents::new(event_callback));
        let core = SessionCore::new(config, dialect, events.clone());
        Ok(Self { core, events })
    }
}

impl Drop for GrblSession {
    fn drop(&mut self) {
        // Stop the background tasks even when the session is dropped
        // without disconnect(); otherwise they orphan on the shared
        // runtime and marshal events onto a possibly-closed loop.
        self.events.close();
        self.core.abort_background_tasks_now();
    }
}
