//! The `OctoPrintSession` PyO3 wrapper: the server's REST API and
//! SockJS WebSocket, exposed through the uniform session surface.

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::octoprint::session::{OctoPrintConfig, OctoPrintCore};
use crate::python::core::json::value_to_py;
use crate::python::core::types::DeviceState;
use crate::python::events::PyEvents;
use crate::python::grbl::session::{get_f64, get_i64, get_str};
use crate::python::pyfuture::{none_obj, spawn_future};

fn build_config(dict: Option<&Bound<'_, PyDict>>) -> PyResult<OctoPrintConfig> {
    let host = dict
        .and_then(|dict| get_str(dict, "host").ok().flatten())
        .unwrap_or_default();
    let mut config = OctoPrintConfig::new(&host);
    let Some(dict) = dict else {
        return Ok(config);
    };
    if let Some(v) = get_i64(dict, "port")? {
        config.port = v.max(0) as u16;
    }
    if let Some(v) = get_str(dict, "path").ok().flatten() {
        config.path_prefix =
            crate::octoprint::session::normalize_path_prefix(&v);
    }
    if let Some(v) = get_str(dict, "api_key").ok().flatten() {
        config.api_key = v;
    }
    if let Some(v) = get_f64(dict, "request_timeout")? {
        config.request_timeout = v;
    }
    if let Some(v) = get_f64(dict, "connect_timeout")? {
        config.connect_timeout = v;
    }
    if let Some(v) = get_f64(dict, "poll_interval")? {
        config.poll_interval = v;
    }
    if let Some(v) = get_f64(dict, "reconnect_delay")? {
        config.reconnect_delay = v;
    }
    if let Some(v) = get_f64(dict, "ws_ping_interval")? {
        config.ws_ping_interval = v;
    }
    Ok(config)
}

/// An OctoPrint server session: job submission and live printer
/// state over the REST API and SockJS WebSocket.
///
/// All async methods return asyncio futures; a typed `SessionEvent`
/// is delivered to `event_callback(event)` on the event loop thread.
/// Operations the OctoPrint API cannot perform (e.g. writing
/// firmware settings) raise `NotImplementedError`.
#[gen_stub_pyclass(module = "raydriver.octoprint")]
#[pyclass(module = "raydriver.octoprint")]
pub struct OctoPrintSession {
    core: Arc<OctoPrintCore>,
    events: Arc<PyEvents>,
}

impl OctoPrintSession {
    fn build(
        config: OctoPrintConfig,
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let events = Arc::new(PyEvents::new(event_callback));
        let core = OctoPrintCore::new(config, events.clone());
        Ok(Self { core, events })
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl OctoPrintSession {
    #[new]
    #[pyo3(signature = (config, dialect=None, event_callback=None))]
    fn new(
        #[gen_stub(override_type(
            type_repr = "typing.Optional[typing.Union[dict, config.SessionConfig]]",
            imports = ("typing", "raydriver.config")
        ))]
        config: Option<Bound<'_, PyAny>>,

        #[gen_stub(override_type(
            type_repr = "typing.Optional[dict]",
            imports = ("typing",)
        ))]
        dialect: Option<Bound<'_, PyDict>>,

        #[gen_stub(override_type(
            type_repr = "typing.Optional[collections.abc.Callable[[core.SessionEvent], typing.Any]]",
            imports = ("collections.abc", "typing", "raydriver.core")
        ))]
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let _ = dialect;
        let config = crate::python::config::config_dict(config.as_ref())?;
        let config = build_config(config.as_ref())?;
        Self::build(config, event_callback)
    }

    /// Fetch the census payloads (the parsed ``/api/version`` and
    /// ``/api/printer`` responses) over the session, so local and
    /// remote probing use the same code path.
    fn probe_info<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            let (version, printer) = core.probe_info().await?;
            Python::attach(|py| {
                let pair =
                    (value_to_py(py, &version)?, value_to_py(py, &printer)?);
                Ok(PyList::new(py, [pair.0, pair.1])?.unbind().into_any())
            })
        })
    }

    /// Run the device census over the session: the parsed
    /// `/api/version` and `/api/printer` responses, interpreted
    /// into the structured census dict. Local and remote sessions
    /// probe identically.
    fn probe_device<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            let (version, printer) = core.probe_info().await?;
            let census = crate::octoprint::probe::describe(&version, &printer);
            Python::attach(|py| {
                crate::python::core::json::value_to_py(py, &census)
            })
        })
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

    /// Stop all background tasks and close the connection.
    fn disconnect<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.cleanup().await;
            Ok(Python::attach(none_obj))
        })
    }

    /// Send a single command and await the server's HTTP
    /// acknowledgement.  The API exposes no per-line response text,
    /// so the result is always empty.
    fn execute_interactive_command<'py>(
        &self,
        py: Python<'py>,
        command: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            let lines = core.execute_interactive_command(&command).await?;
            Python::attach(|py| {
                Ok(lines.into_pyobject(py)?.unbind().into_any())
            })
        })
    }

    /// Send a single command without awaiting further state.
    fn execute_command<'py>(
        &self,
        py: Python<'py>,
        command: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.execute_command(&command).await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Upload a G-code string, start it and await the server's
    /// terminal print event.  Every op index in *op_line_map* is
    /// reported to *progress_callback* when the job starts (the
    /// protocol carries no per-op progress).
    #[pyo3(signature = (gcode, op_line_map=None, op_estimates=None,
                        progress_callback=None))]
    #[allow(clippy::too_many_arguments)]
    fn run<'py>(
        &self,
        py: Python<'py>,
        gcode: String,
        op_line_map: Option<HashMap<usize, i64>>,
        op_estimates: Option<Vec<f64>>,
        progress_callback: Option<Py<PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        let events = self.events.clone();
        let op_map = op_line_map.unwrap_or_default();
        let estimates = op_estimates.unwrap_or_default();
        spawn_future(py, async move {
            events.set_progress_callback(progress_callback);
            core.run(&gcode, op_map, estimates).await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Send a raw G-code string as printer commands.
    fn run_raw<'py>(
        &self,
        py: Python<'py>,
        machine_code: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.run_raw(&machine_code).await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Home all axes (`axes` is None) or the named axes.
    #[pyo3(signature = (axes=None))]
    fn home<'py>(
        &self,
        py: Python<'py>,
        axes: Option<Vec<String>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.home(axes.as_deref()).await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Move to an absolute position (mm); `speed` in mm/min.
    #[pyo3(signature = (x, y, z=None, speed=None))]
    fn move_to<'py>(
        &self,
        py: Python<'py>,
        x: f64,
        y: f64,
        z: Option<f64>,
        speed: Option<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.move_to(x, y, z, speed).await?;
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
            core.jog(Some(speed), &deltas).await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Pause (true) or resume (false) the running job.
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

    /// Cancel the running job.
    #[pyo3(signature = (emergency=false))]
    fn cancel<'py>(
        &self,
        py: Python<'py>,
        emergency: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _ = emergency;
        let core = self.core.clone();
        spawn_future(py, async move {
            core.cancel(false).await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Select a tool by sending its tool-change command.
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

    /// Clear the printer's error state (M999) and reset locally.
    fn clear_alarm<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.clear_alarm().await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Set a work coordinate offset via a G10 L2 command.  Raises
    /// `ValueError` for slots the server model does not know.
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

    /// Read the WCS offsets; always empty (the server API exposes
    /// none).
    fn read_wcs_offsets<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            let offsets = core.read_wcs_offsets().await?;
            Python::attach(|py| {
                Ok(offsets.into_pyobject(py)?.unbind().into_any())
            })
        })
    }

    /// Read the G-code parser state; always `None` (the server API
    /// exposes none).
    fn read_parser_state<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            let state = core.read_parser_state().await?;
            Python::attach(|py| {
                Ok(state.into_pyobject(py)?.unbind().into_any())
            })
        })
    }

    /// Run a probe cycle (G38.2); OctoPrint does not report probe
    /// results, so the result is always `None`.
    fn run_probe_cycle<'py>(
        &self,
        py: Python<'py>,
        axis_letter: String,
        max_travel: f64,
        feed_rate: f64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            let pos = core
                .run_probe_cycle(&axis_letter, max_travel, feed_rate)
                .await?;
            Python::attach(|py| Ok(pos.into_pyobject(py)?.unbind().into_any()))
        })
    }

    /// Read the firmware settings; always empty (the server API
    /// exposes none).
    fn read_settings<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            let settings = core.read_settings().await?;
            Python::attach(|py| {
                Ok(settings.into_pyobject(py)?.unbind().into_any())
            })
        })
    }

    /// Raise: the server API cannot write firmware settings.
    fn write_setting<'py>(
        &self,
        py: Python<'py>,
        key: String,
        value: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.write_setting(&key, &value).await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// The unit system the dialect works in; the server API exposes
    /// no unit configuration, so this is always `None`.
    fn detect_unit_system<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            let unit = core.detect_unit_system().await?;
            Python::attach(|py| Ok(unit.into_pyobject(py)?.unbind().into_any()))
        })
    }

    /// Accept dialect command templates; commands are sent as
    /// structured JSON, so nothing is stored.
    fn update_dialect(&self, dialect: Bound<'_, PyDict>) -> PyResult<()> {
        let _ = dialect;
        Ok(())
    }

    /// Current snapshot of the printer state.
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
            "OctoPrintSession(resource_uri={:?}, job_running={})",
            self.core.resource_uri(),
            self.core.is_job_running(),
        )
    }
}
