//! The `SmoothieSession` PyO3 wrapper.

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::core::dialect::Dialect;
use crate::core::transport::TransportKind;
use crate::python::core::types::DeviceState;
use crate::python::events::PyEvents;
use crate::python::grbl::session::{
    get_bool, get_f64, parse_dialect, transport_kind_from_config,
};
use crate::python::pyfuture::{none_obj, spawn_future};
use crate::smoothie::session::{SmoothieConfig, SmoothieCore};

pub(crate) const MODULE_DOC: &str = "\
The Smoothieware session: one line at a time, waiting for each \
ok/error acknowledgement, with Grbl-style status reports.
";

pyo3_stub_gen::module_doc!("raydriver.smoothie", "{}", MODULE_DOC);

fn build_smoothie_config(
    kind: TransportKind,
    dict: Option<&Bound<'_, PyDict>>,
) -> PyResult<SmoothieConfig> {
    let mut config = SmoothieConfig::new(kind);
    let Some(dict) = dict else {
        return Ok(config);
    };
    if let Some(v) = get_bool(dict, "poll_status_while_running")? {
        config.poll_status_while_running = v;
    }
    if let Some(v) = get_f64(dict, "command_timeout")? {
        config.command_timeout = v;
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
    if let Some(v) = get_f64(dict, "safety_shutdown_delay")? {
        config.safety_shutdown_delay = v;
    }
    Ok(config)
}

/// A Smoothieware device session using ping-pong communication.
///
/// Sends one command at a time and waits for `ok` before
/// proceeding; status reports are Grbl-style `<...>` replies to `?`.
///
/// All async methods return asyncio futures; a typed `SessionEvent`
/// is delivered to `event_callback(event)` on the event loop thread.
#[gen_stub_pyclass]
#[pyclass(module = "raydriver.smoothie")]
pub struct SmoothieSession {
    core: Arc<SmoothieCore>,
    events: Arc<PyEvents>,
}

#[gen_stub_pymethods]
#[pymethods]
impl SmoothieSession {
    #[new]
    #[pyo3(signature = (config, dialect=None, event_callback=None))]
    fn new(
        config: Option<Bound<'_, PyDict>>,
        dialect: Option<Bound<'_, PyDict>>,
        #[gen_stub(override_type(
            type_repr = "typing.Optional[collections.abc.Callable[[core.SessionEvent], typing.Any]]",
            imports = ("collections.abc", "typing", "raydriver.core")
        ))]
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let kind = transport_kind_from_config(config.as_ref())?;
        let session_config = build_smoothie_config(kind, config.as_ref())?;
        Self::build(session_config, dialect, event_callback)
    }

    /// Build a session around a [`MockTransport`] (for tests).
    #[staticmethod]
    #[pyo3(signature = (transport, config=None, dialect=None, event_callback=None))]
    fn with_transport(
        transport: PyRef<'_, crate::python::core::mock::MockTransport>,
        config: Option<Bound<'_, PyDict>>,
        dialect: Option<Bound<'_, PyDict>>,
        #[gen_stub(override_type(
            type_repr = "typing.Optional[collections.abc.Callable[[core.SessionEvent], typing.Any]]",
            imports = ("collections.abc", "typing", "raydriver.core")
        ))]
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let kind = TransportKind::Mock(transport.inner.clone());
        let session_config = build_smoothie_config(kind, config.as_ref())?;
        Self::build(session_config, dialect, event_callback)
    }

    /// Open the connection and start the background tasks.
    fn connect<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.events.capture_loop(py)?;
        let core = self.core.clone();
        spawn_future(py, async move {
            core.connect().await;
            Ok(Python::attach(none_obj))
        })
    }

    /// Stop all background tasks and close the transport.
    fn disconnect<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.cleanup().await;
            Ok(Python::attach(none_obj))
        })
    }

    /// Send a command and await its full response.
    fn execute_interactive_command<'py>(
        &self,
        py: Python<'py>,
        command: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            let lines = core.execute_interactive_command(command).await?;
            Python::attach(|py| {
                Ok(lines.into_pyobject(py)?.unbind().into_any())
            })
        })
    }

    /// Queue a command and await its response (identical to
    /// `execute_interactive_command`).
    fn execute_command<'py>(
        &self,
        py: Python<'py>,
        command: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            let lines = core.execute_command(command).await?;
            Python::attach(|py| {
                Ok(lines.into_pyobject(py)?.unbind().into_any())
            })
        })
    }

    /// Stream a G-code program using the ping-pong protocol.
    ///
    /// `op_line_map` maps G-code line indices to op indices for
    /// progress reporting.
    #[pyo3(signature = (gcode, op_line_map=None, progress_callback=None))]
    fn run<'py>(
        &self,
        py: Python<'py>,
        gcode: String,
        op_line_map: Option<HashMap<usize, i64>>,
        progress_callback: Option<Py<PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        let events = self.events.clone();
        let op_map = op_line_map.unwrap_or_default();
        spawn_future(py, async move {
            events.set_progress_callback(progress_callback);
            core.run(&gcode, op_map).await;
            Ok(Python::attach(none_obj))
        })
    }

    /// Execute a raw G-code string line by line.
    fn run_raw<'py>(
        &self,
        py: Python<'py>,
        machine_code: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.run_raw(&machine_code).await;
            Ok(Python::attach(none_obj))
        })
    }

    /// Pause (feed hold) or resume the device.
    #[pyo3(signature = (hold=true))]
    fn set_hold<'py>(
        &self,
        py: Python<'py>,
        hold: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.set_hold(hold).await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Abort the running job: Ctrl+C and safety shutdown.
    #[pyo3(signature = (emergency=false))]
    fn cancel<'py>(
        &self,
        py: Python<'py>,
        emergency: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.cancel(emergency).await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Home all axes (`axes` is None) or the named axes (e.g.
    /// `["X"]`).
    #[pyo3(signature = (axes=None))]
    fn home<'py>(
        &self,
        py: Python<'py>,
        axes: Option<Vec<String>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.home(axes).await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Move to an absolute position (mm) at `speed` (mm/min).
    fn move_to<'py>(
        &self,
        py: Python<'py>,
        speed: f64,
        x: f64,
        y: f64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.move_to(speed, x, y).await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Jog by relative deltas (mm) at `speed` (mm/min).
    fn jog<'py>(
        &self,
        py: Python<'py>,
        speed: f64,
        deltas: Vec<(String, f64)>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.jog(speed, &deltas).await?;
            Ok(Python::attach(none_obj))
        })
    }

    fn select_tool<'py>(
        &self,
        py: Python<'py>,
        tool_number: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.select_tool(tool_number).await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Clear an alarm state with the dialect's clear command.
    fn clear_alarm<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.clear_alarm().await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Set laser power.  `power` is the absolute S value; `None`
    /// (or zero) turns the laser off.
    #[pyo3(signature = (power=None))]
    fn set_power<'py>(
        &self,
        py: Python<'py>,
        power: Option<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.set_power(power).await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Set laser power for focus mode; `None` turns the laser off.
    #[pyo3(signature = (power=None))]
    fn set_focus_power<'py>(
        &self,
        py: Python<'py>,
        power: Option<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.set_focus_power(power).await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Set a WCS offset via the dialect's `set_wcs_offset` template.
    fn set_wcs_offset<'py>(
        &self,
        py: Python<'py>,
        wcs_slot: String,
        x: f64,
        y: f64,
        z: Option<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.set_wcs_offset(&wcs_slot, x, y, z).await?;
            Ok(Python::attach(none_obj))
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
    fn job_running(&self) -> bool {
        self.core.is_job_running()
    }

    #[getter]
    fn resource_uri(&self) -> Option<String> {
        self.core.resource_uri()
    }

    fn __repr__(&self) -> String {
        format!(
            "SmoothieSession(resource_uri={:?}, job_running={})",
            self.resource_uri(),
            self.job_running()
        )
    }
}

impl SmoothieSession {
    fn build(
        config: SmoothieConfig,
        dialect: Option<Bound<'_, PyDict>>,
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let dialect: Dialect = parse_dialect(dialect.as_ref())?;
        let events = Arc::new(PyEvents::new(event_callback));
        let core = SmoothieCore::new(config, dialect, events.clone());
        Ok(Self { core, events })
    }
}
