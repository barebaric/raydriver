//! The `EpilogDriver` PyO3 wrapper: LPD job submission, exposed
//! through the uniform session surface plus the protocol-specific
//! `upload` that carries the encoder's byte stream.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyDict};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::core::driver::{CancelMode, Driver};
use crate::epilog::driver::{EpilogConfig, EpilogDriver};
use crate::python::core::types::DeviceState;
use crate::python::events::PyEvents;
use crate::python::grbl::driver::{get_f64, get_i64, get_str};
use crate::python::pyfuture::{
    spawn_future, spawn_future_with_cancel, PyAwait,
};

fn build_config(dict: Option<&Bound<'_, PyDict>>) -> PyResult<EpilogConfig> {
    let host = dict
        .and_then(|dict| get_str(dict, "host").ok().flatten())
        .unwrap_or_default();
    let mut config = EpilogConfig::new(&host);
    let Some(dict) = dict else {
        return Ok(config);
    };
    if let Some(v) = get_i64(dict, "port")? {
        config.port = v.clamp(1, u16::MAX.into()) as u16;
    }
    if let Some(v) = get_str(dict, "queue").ok().flatten() {
        config.queue = v;
    }
    if let Some(v) = get_str(dict, "submit_host").ok().flatten() {
        config.submit_host = v;
    }
    if let Some(v) = get_str(dict, "user").ok().flatten() {
        config.user = v;
    }
    if let Some(v) = get_f64(dict, "timeout")? {
        config.timeout = v;
    }
    Ok(config)
}

/// An Epilog laser session: prints jobs through the machine's LPD
/// print queue.
///
/// All async methods return asyncio futures; a typed `DriverEvent`
/// is delivered to `event_callback(event)` on the event loop thread.
/// The machine has no status channel, so most session operations
/// raise `NotImplementedError`; job submission uses `upload`.
#[gen_stub_pyclass(module = "raydriver.epilog")]
#[pyclass(name = "EpilogDriver", module = "raydriver.epilog")]
pub struct PyEpilogDriver {
    core: Arc<EpilogDriver>,
    events: Arc<PyEvents>,
}

impl PyEpilogDriver {
    fn build(config: EpilogConfig, event_callback: Option<Py<PyAny>>) -> Self {
        let events = Arc::new(PyEvents::new(event_callback));
        let core = Arc::new(EpilogDriver::new(config, events.clone()));
        Self { core, events }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyEpilogDriver {
    #[new]
    #[pyo3(signature = (config, dialect=None, event_callback=None))]
    fn new(
        #[gen_stub(override_type(
            type_repr = "typing.Optional[dict]",
            imports = ("typing",)
        ))]
        config: Option<Bound<'_, PyAny>>,

        #[gen_stub(override_type(
            type_repr = "typing.Optional[dict]",
            imports = ("typing",)
        ))]
        dialect: Option<Bound<'_, PyDict>>,

        #[gen_stub(override_type(
            type_repr = "typing.Optional[collections.abc.Callable[[core.DriverEvent], builtins.object]]",
            imports = ("builtins", "collections.abc", "typing", "raydriver.core")
        ))]
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let _ = dialect;
        let config = crate::python::config::config_dict(config.as_ref())?;
        let config = build_config(config.as_ref())?;
        Ok(Self::build(config, event_callback))
    }

    /// Upload a complete print job and wait until the machine accepts
    /// it.
    ///
    /// :param payload: the encoder's byte stream for this job.
    /// :param title: the job name shown in the machine's job list.
    fn upload<'py>(
        &self,
        py: Python<'py>,
        #[gen_stub(override_type(
            type_repr = "typing.Union[builtins.bytes, builtins.bytearray]",
            imports = ("builtins", "typing")
        ))]
        payload: &Bound<'_, PyAny>,
        title: String,
    ) -> PyResult<PyAwait<'py, ()>> {
        let bytes: Vec<u8> = if let Ok(ba) = payload.cast::<PyByteArray>() {
            ba.to_vec()
        } else if let Ok(b) = payload.cast::<PyBytes>() {
            b.as_bytes().to_vec()
        } else {
            return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "payload must be bytes or bytearray",
            ));
        };
        self.events.capture_loop(py)?;
        let core = self.core.clone();
        let cancel_core = core.clone();
        spawn_future_with_cancel(
            py,
            async move {
                core.upload(bytes, title).await?;
                Ok(())
            },
            move || {
                crate::python::runtime::handle().spawn(async move {
                    let _ =
                        Driver::cancel(&*cancel_core, CancelMode::Immediate)
                            .await;
                });
            },
        )
    }

    /// Probe the LPD port without submitting anything.
    fn probe<'py>(&self, py: Python<'py>) -> PyAwait<'py, ()> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.probe().await?;
            Ok(())
        })
        .expect("probe future")
    }

    /// Current snapshot of the machine state.
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
            "EpilogDriver(resource_uri={:?}, job_running={})",
            self.core.resource_uri(),
            self.core.is_job_running(),
        )
    }
}

use crate::python::session_api::uniform_session_api;

uniform_session_api!(PyEpilogDriver);
