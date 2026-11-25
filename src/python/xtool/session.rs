//! The `XtoolSession` PyO3 wrapper: thin async methods delegating to
//! the Rust session core.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::core::dialect::Dialect;
use crate::core::driver::{CancelMode, Driver};
use crate::python::core::types::DeviceState;
use crate::python::events::PyEvents;
// The config/dialect dict helpers are shared across the protocol
// bindings and live with the largest consumer (grbl), mirroring how
// the smoothie and marlin bindings reuse them.
use crate::python::grbl::session::{get_f64, get_i64, get_str, parse_dialect};
use crate::python::pyfuture::{spawn_future, PyAwait};
use crate::xtool::session::{XtoolConfig, XtoolSession};

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
/// All async methods return asyncio futures; a typed `DriverEvent`
/// is delivered to `event_callback(event)` on the event loop thread.
#[gen_stub_pyclass]
#[pyclass(name = "XtoolSession", module = "raydriver.xtool")]
pub struct PyXtoolSession {
    core: Arc<XtoolSession>,
    events: Arc<PyEvents>,
}

impl PyXtoolSession {
    fn build(
        config: XtoolConfig,
        dialect: Dialect,
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let events = Arc::new(PyEvents::new(event_callback));
        let core = XtoolSession::new(config, dialect, events.clone());
        Ok(Self { core, events })
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyXtoolSession {
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
            type_repr = "typing.Optional[collections.abc.Callable[[core.DriverEvent], builtins.object]]",
            imports = ("builtins", "collections.abc", "typing", "raydriver.core")
        ))]
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let config = crate::python::config::config_dict(config.as_ref())?;
        let config = build_config(config.as_ref())?;
        let dialect = parse_dialect(dialect.as_ref())?;
        Self::build(config, dialect, event_callback)
    }
    /// Execute a multi-line block of G-code via POST /cmd.
    fn execute_block<'py>(
        &self,
        py: Python<'py>,
        block: String,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.execute_block(block).await?;
            Ok(())
        })
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

use crate::python::session_api::uniform_session_api;

uniform_session_api!(PyXtoolSession);
