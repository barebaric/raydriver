use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::python::core::types::DeviceState;
use crate::python::events::PyEvents;
use crate::python::grbl::session::{get_bool, get_f64, get_i64, get_str};
use crate::python::pyfuture::{none_obj, spawn_future};
use crate::xtool::m2::{XtoolM2Config, XtoolM2Core};

fn build_config(dict: Option<&Bound<'_, PyDict>>) -> PyResult<XtoolM2Config> {
    let host = dict
        .and_then(|dict| get_str(dict, "host").ok().flatten())
        .unwrap_or_default();
    let mut config = XtoolM2Config::new(&host);
    let Some(dict) = dict else {
        return Ok(config);
    };
    if let Some(value) = get_i64(dict, "port")? {
        config.port = value.clamp(0, u16::MAX as i64) as u16;
    }
    if let Some(value) = get_f64(dict, "request_timeout")? {
        config.request_timeout = value;
    }
    if let Some(value) = get_f64(dict, "connect_timeout")? {
        config.connect_timeout = value;
    }
    if let Some(value) = get_f64(dict, "reconnect_delay")? {
        config.reconnect_delay = value;
    }
    if let Some(value) = get_f64(dict, "poll_interval")? {
        config.poll_interval = value;
    }
    if let Some(value) = get_bool(dict, "tls_allow_invalid_certs")? {
        config.tls_allow_invalid_certs = value;
    }
    Ok(config)
}

#[gen_stub_pyclass]
/// Monitor and control an xTool M2 over its WS-V2 TLS service.
/// This session does not upload G-code jobs. `start` controls a job
/// that is already staged on the device, such as a job sent by xTool
/// Studio.
#[pyclass(module = "raydriver.xtool")]
pub struct XtoolM2Session {
    core: Arc<XtoolM2Core>,
    events: Arc<PyEvents>,
}

impl XtoolM2Session {
    fn build(config: XtoolM2Config, event_callback: Option<Py<PyAny>>) -> Self {
        let events = Arc::new(PyEvents::new(event_callback));
        let core = XtoolM2Core::new(config, events.clone());
        Self { core, events }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl XtoolM2Session {
    #[new]
    #[pyo3(signature = (config, event_callback=None))]
    /// Create an M2 session for the configured host.
    fn new(
        config: Option<Bound<'_, PyDict>>,
        #[gen_stub(override_type(
            type_repr = "typing.Optional[collections.abc.Callable[[core.SessionEvent], typing.Any]]",
            imports = ("collections.abc", "typing", "raydriver.core")
        ))]
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self::build(build_config(config.as_ref())?, event_callback))
    }

    /// Connect and start monitoring the M2.
    fn connect<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.events.capture_loop(py)?;
        let core = self.core.clone();
        spawn_future(py, async move {
            core.connect().await;
            Ok(Python::attach(none_obj))
        })
    }

    /// Stop monitoring and close the connection.
    fn disconnect<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.disconnect().await;
            Ok(Python::attach(none_obj))
        })
    }

    /// Start a job already staged on the device.
    fn start<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.start().await?;
            Ok(Python::attach(none_obj))
        })
    }

    #[pyo3(signature = (hold=true))]
    /// Pause the current job, or resume it when `hold` is false.
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

    /// Cancel the current job.
    fn cancel<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.cancel().await?;
            Ok(Python::attach(none_obj))
        })
    }

    /// Home the laser head.
    fn home<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.home().await?;
            Ok(Python::attach(none_obj))
        })
    }

    #[pyo3(signature = (x, y, speed=18000.0))]
    /// Move to an absolute XY coordinate in millimeters.
    fn move_to<'py>(
        &self,
        py: Python<'py>,
        x: f64,
        y: f64,
        speed: f64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.move_to(x, y, speed).await?;
            Ok(Python::attach(none_obj))
        })
    }

    #[pyo3(signature = (dx=0.0, dy=0.0, dz=0.0, speed=18000.0))]
    /// Move by relative XYZ deltas in millimeters.
    fn jog<'py>(
        &self,
        py: Python<'py>,
        dx: f64,
        dy: f64,
        dz: f64,
        speed: f64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.jog(dx, dy, dz, speed).await?;
            Ok(Python::attach(none_obj))
        })
    }

    #[getter]
    /// Current snapshot of the machine state.
    fn state(&self) -> DeviceState {
        DeviceState::new(self.core.state())
    }

    #[getter]
    /// Whether the state currently indicates an active or held job.
    fn is_job_running(&self) -> bool {
        self.core.is_job_running()
    }

    #[getter]
    /// WebSocket URI of the configured device.
    fn resource_uri(&self) -> Option<String> {
        self.core.resource_uri()
    }

    fn __repr__(&self) -> String {
        format!(
            "XtoolM2Session(resource_uri={:?}, job_running={})",
            self.core.resource_uri(),
            self.core.is_job_running(),
        )
    }
}

impl Drop for XtoolM2Session {
    fn drop(&mut self) {
        self.core.abort_background_tasks_now();
    }
}
