//! The `SmoothieSession` PyO3 wrapper.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::core::dialect::Dialect;
use crate::core::driver::{CancelMode, Driver};
use crate::core::transport::TransportKind;
use crate::python::core::types::DeviceState;
use crate::python::events::PyEvents;
use crate::python::grbl::session::{
    get_bool, get_f64, parse_dialect, transport_kind_from_config,
};
use crate::python::pyfuture::{spawn_future, PyAwait};
use crate::python::smoothie::probe::SmoothieDeviceInfo;
use crate::smoothie::session::{SmoothieConfig, SmoothieSession};

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
/// All async methods return asyncio futures; a typed `DriverEvent`
/// is delivered to `event_callback(event)` on the event loop thread.
#[gen_stub_pyclass]
#[pyclass(name = "SmoothieSession", module = "raydriver.smoothie")]
pub struct PySmoothieSession {
    core: Arc<SmoothieSession>,
    events: Arc<PyEvents>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PySmoothieSession {
    #[new]
    #[pyo3(signature = (config, dialect=None, event_callback=None))]
    fn new(
        #[gen_stub(override_type(
            type_repr = "typing.Optional[typing.Union[dict, config.SmoothieConfig]]",
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
        let kind = transport_kind_from_config(config.as_ref())?;
        let session_config = build_smoothie_config(kind, config.as_ref())?;
        Self::build(session_config, dialect, event_callback)
    }

    /// Build a session around a [`MockTransport`] (for tests).
    #[staticmethod]
    #[pyo3(signature = (transport, config=None, dialect=None, event_callback=None))]
    fn with_transport(
        transport: PyRef<'_, crate::python::core::mock::MockTransport>,
        #[gen_stub(override_type(
            type_repr = "typing.Optional[typing.Union[dict, config.SmoothieConfig]]",
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
        let kind = TransportKind::Mock(transport.inner.clone());
        let session_config = build_smoothie_config(kind, config.as_ref())?;
        Self::build(session_config, dialect, event_callback)
    }
    /// Send a command and await its full response.
    /// Run the device census: query ``version`` and the
    /// ``config-get`` motion keys, and return the structured result
    /// as a dict.
    fn probe_device<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<PyAwait<'py, SmoothieDeviceInfo>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            let mut replies: Vec<Vec<String>> = Vec::new();
            for command in crate::smoothie::probe::CENSUS_COMMANDS {
                replies.push(core.execute_interactive_command(command).await?);
            }
            Ok(SmoothieDeviceInfo {
                inner: crate::smoothie::probe::describe(
                    &replies[0],
                    &replies[1],
                    &replies[2],
                    &replies[3],
                    &replies[4],
                    &replies[5],
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
            "SmoothieSession(resource_uri={:?}, job_running={})",
            self.resource_uri(),
            self.job_running()
        )
    }
}

impl PySmoothieSession {
    fn build(
        config: SmoothieConfig,
        dialect: Option<Bound<'_, PyDict>>,
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let dialect: Dialect = parse_dialect(dialect.as_ref())?;
        let events = Arc::new(PyEvents::new(event_callback));
        let core = SmoothieSession::new(config, dialect, events.clone());
        Ok(Self { core, events })
    }
}

impl Drop for PySmoothieSession {
    fn drop(&mut self) {
        // Stop the background tasks even when the session is dropped
        // without disconnect(); otherwise they orphan on the shared
        // runtime and accumulate across sessions.
        self.events.close();
        self.core.abort_background_tasks_now();
    }
}

use crate::python::session_api::uniform_session_api;

uniform_session_api!(PySmoothieSession);
