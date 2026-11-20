//! The `XtoolSession` PyO3 wrapper: thin async methods delegating to
//! the Rust session core.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::core::dialect::Dialect;
use crate::python::core::types::DeviceState;
use crate::python::events::PyEvents;
// The config/dialect dict helpers are shared across the protocol
// bindings and live with the largest consumer (grbl), mirroring how
// the smoothie and marlin bindings reuse them.
use crate::python::grbl::session::{get_f64, get_i64, get_str, parse_dialect};
use crate::python::pyfuture::{none_obj, spawn_future};
use crate::xtool::session::{XtoolConfig, XtoolCore};

fn build_config(dict: Option<&Bound<'_, PyDict>>) -> PyResult<XtoolConfig> {
    let host = dict
        .and_then(|dict| get_str(dict, "host").ok().flatten())
        .unwrap_or_default();
    let mut config = XtoolConfig::new(&host);
    let Some(dict) = dict else {
        return Ok(config);
    };
    if let Some(v) = get_i64(dict, "http_port")? {
        config.http_port = v.max(0) as u16;
    }
    if let Some(v) = get_i64(dict, "ws_port")? {
        config.ws_port = v.max(0) as u16;
    }
    if let Some(v) = get_f64(dict, "request_timeout")? {
        config.request_timeout = v;
    }
    if let Some(v) = get_f64(dict, "connect_timeout")? {
        config.connect_timeout = v;
    }
    if let Some(v) = get_f64(dict, "reconnect_delay")? {
        config.reconnect_delay = v;
    }
    if let Some(v) = get_f64(dict, "homing_timeout")? {
        config.homing_timeout = v;
    }
    if let Some(v) = get_f64(dict, "job_start_timeout")? {
        config.job_start_timeout = v;
    }
    if let Some(v) = get_f64(dict, "poll_interval")? {
        config.poll_interval = v;
    }
    Ok(config)
}

/// An xTool device session: connection lifecycle, interactive
/// commands and job execution over the proprietary WiFi API,
/// entirely in Rust.
///
/// All async methods return asyncio futures; a typed `SessionEvent`
/// is delivered to `event_callback(event)` on the event loop thread.
#[gen_stub_pyclass]
#[pyclass(module = "raydriver.xtool")]
pub struct XtoolSession {
    core: Arc<XtoolCore>,
    events: Arc<PyEvents>,
}

impl XtoolSession {
    fn build(
        config: XtoolConfig,
        dialect: Dialect,
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let events = Arc::new(PyEvents::new(event_callback));
        let core = XtoolCore::new(config, dialect, events.clone());
        Ok(Self { core, events })
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl XtoolSession {
    #[new]
    #[pyo3(signature = (config, dialect=None, event_callback=None))]
    fn new(
        #[gen_stub(override_type(
            type_repr = "typing.Optional[typing.Union[dict, config.XtoolConfig]]",
            imports = ("typing", "raydriver.config")
        ))]
        config: Option<Bound<'_, PyAny>>,

        dialect: Option<Bound<'_, PyDict>>,
        #[gen_stub(override_type(
            type_repr = "typing.Optional[collections.abc.Callable[[core.SessionEvent], typing.Any]]",
            imports = ("collections.abc", "typing", "raydriver.core")
        ))]
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let config = crate::python::config::config_dict(config.as_ref())?;
        let config = build_config(config.as_ref())?;
        let dialect = parse_dialect(dialect.as_ref())?;
        Self::build(config, dialect, event_callback)
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
        let events = self.events.clone();
        spawn_future(py, async move {
            core.cleanup().await;
            events.quiesce();
            Ok(Python::attach(none_obj))
        })
    }

    /// Send a single G-code line and await the device's HTTP
    /// acknowledgement.  The protocol has no per-line response text,
    /// so the result is always empty.
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

    /// Queue a single G-code line without failing on device-side
    /// rejections; the command status carries the error.
    fn execute_command<'py>(
        &self,
        py: Python<'py>,
        command: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.execute_command(command).await;
            Ok(Python::attach(none_obj))
        })
    }

    /// Execute a multi-line block of G-code via POST /cmd.
    fn execute_block<'py>(
        &self,
        py: Python<'py>,
        block: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.execute_block(block).await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Upload a G-code job, start it and await the machine's
    /// job-state events until it reports idle again.
    fn run<'py>(
        &self,
        py: Python<'py>,
        gcode: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.run(&gcode).await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Upload a raw G-code string, start it and await completion.
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

    /// Run the homing cycle and await the machine going idle again.
    fn home<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.home().await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Move to an absolute position (mm) at `speed` (mm/min).
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
    #[pyo3(signature = (speed, deltas=None))]
    fn jog<'py>(
        &self,
        py: Python<'py>,
        speed: f64,
        deltas: Option<Vec<(String, f64)>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.jog(speed, deltas.unwrap_or_default()).await?;
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

    /// Stop the running job and send the safety-off commands.
    #[pyo3(signature = (emergency=false))]
    fn cancel<'py>(
        &self,
        py: Python<'py>,
        emergency: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.cancel(emergency).await;
            Ok(Python::attach(none_obj))
        })
    }

    /// Set laser power.  `power` is the absolute S value; `None`
    /// turns the laser off.
    #[pyo3(signature = (power=None))]
    fn set_power<'py>(
        &self,
        py: Python<'py>,
        power: Option<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.set_power(power).await;
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
            core.set_focus_power(power).await;
            Ok(Python::attach(none_obj))
        })
    }

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
    fn is_job_running(&self) -> bool {
        self.core.is_job_running()
    }

    #[getter]
    fn resource_uri(&self) -> Option<String> {
        self.core.resource_uri()
    }

    fn __repr__(&self) -> String {
        format!(
            "XtoolSession(resource_uri={:?}, job_running={})",
            self.core.resource_uri(),
            self.core.is_job_running(),
        )
    }
}
