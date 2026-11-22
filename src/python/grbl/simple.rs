//! The `GrblSimpleSession` PyO3 wrapper: the minimal ping-pong
//! session exposed to Python.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::core::dialect::Dialect;
use crate::core::session::{CancelMode, Session};
use crate::core::transport::TransportKind;
use crate::grbl::simple::{GrblSimpleSession, SimpleConfig};

use crate::python::core::mock::MockTransport;
use crate::python::core::types::DeviceState;
use crate::python::events::PyEvents;
use crate::python::grbl::probe::GrblDeviceInfo;
use crate::python::grbl::session::{
    get_bool, get_f64, parse_dialect, transport_kind_from_config,
};
use crate::python::pyfuture::{spawn_future, PyAwait};

pub(crate) const MODULE_DOC: &str = "\
The minimal ping-pong GRBL session: one line at a time, waiting for \
each ok/error acknowledgement. For hardware where the full driver \
misbehaves.
";

pyo3_stub_gen::module_doc!("raydriver.grbl.simple", "{}", MODULE_DOC);

fn build_simple_config(
    kind: TransportKind,
    dict: Option<&Bound<'_, PyDict>>,
) -> PyResult<SimpleConfig> {
    let mut config = SimpleConfig::new(kind);
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

/// A minimal GRBL device session using ping-pong communication.
///
/// Sends one G-code line at a time and waits for `ok` before
/// proceeding.  No character-counting, no buffer management, no
/// deadlock detection — for hardware where the full
/// [`GrblSession`][raydriver.grbl.GrblSession] misbehaves.
///
/// All async methods return asyncio futures; a typed `SessionEvent`
/// is delivered to `event_callback(event)` on the event loop thread.
#[gen_stub_pyclass]
#[pyclass(name = "GrblSimpleSession", module = "raydriver.grbl.simple")]
pub struct PyGrblSimpleSession {
    core: Arc<GrblSimpleSession>,
    events: Arc<PyEvents>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyGrblSimpleSession {
    #[new]
    #[pyo3(signature = (config, dialect=None, event_callback=None))]
    fn new(
        #[gen_stub(override_type(
            type_repr = "typing.Optional[typing.Union[dict, config.GrblSimpleConfig]]",
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
        let session_config = build_simple_config(kind, config.as_ref())?;
        Self::build(session_config, dialect, event_callback)
    }

    /// Build a session around a [`MockTransport`] (for tests).
    #[staticmethod]
    #[pyo3(signature = (transport, config=None, dialect=None, event_callback=None))]
    fn with_transport(
        transport: PyRef<'_, MockTransport>,
        #[gen_stub(override_type(
            type_repr = "typing.Optional[typing.Union[dict, config.GrblSimpleConfig]]",
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
        let session_config = build_simple_config(kind, config.as_ref())?;
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
    fn job_running(&self) -> bool {
        self.core.is_job_running()
    }

    #[getter]
    fn resource_uri(&self) -> Option<String> {
        self.core.resource_uri()
    }

    fn __repr__(&self) -> String {
        format!(
            "GrblSimpleSession(resource_uri={:?}, job_running={})",
            self.resource_uri(),
            self.job_running()
        )
    }
}

impl PyGrblSimpleSession {
    fn build(
        config: SimpleConfig,
        dialect: Option<Bound<'_, PyDict>>,
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let dialect: Dialect = parse_dialect(dialect.as_ref())?;
        let events = Arc::new(PyEvents::new(event_callback));
        let core = GrblSimpleSession::new(config, dialect, events.clone());
        Ok(Self { core, events })
    }
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.setattr("__doc__", MODULE_DOC)?;
    m.add("__all__", vec!["GrblSimpleSession"])?;
    m.add_class::<PyGrblSimpleSession>()?;
    Ok(())
}

use crate::python::session_api::uniform_session_api;

uniform_session_api!(PyGrblSimpleSession);
