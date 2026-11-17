//! The `raydriver.core.discovery.fingerprints` submodule:
//! reusable fingerprint probes for generic mDNS service types.
//!
//! A fingerprint is an async callable handed to an `MdnsRecognizer`;
//! the network channel calls it with a candidate's address and port.
//! `TcpFingerprint` provides the common case: confirm a raw TCP
//! service by sending an optional command and matching a response
//! line prefix. Protocol-specific command strings stay with the
//! driver that declares them.

use std::time::Duration;

use pyo3::prelude::*;
use pyo3::types::PyBool;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::core::discovery::probe::{probe_tcp, TcpProbe};
use crate::python::pyfuture::spawn_future;

use super::helpers;

/// Confirms a generic TCP service by matching a response line.
///
/// The probe waits *banner_wait* seconds for an unsolicited banner,
/// then (when *probe_command* is set) sends it and waits for the
/// remaining *timeout* seconds. It returns a truthy value when a
/// line starts with *match_prefix*, and `None` otherwise.
#[gen_stub_pyclass(module = "raydriver.core.discovery.fingerprints")]
#[pyclass(
    module = "raydriver.core.discovery.fingerprints",
    frozen,
    skip_from_py_object
)]
#[derive(Clone)]
pub struct TcpFingerprint {
    probe_command: Option<String>,
    match_prefix: String,
    banner_wait: f64,
    timeout: f64,
}

#[gen_stub_pymethods]
#[pymethods]
impl TcpFingerprint {
    #[new]
    #[pyo3(signature = (
        probe_command=None,
        match_prefix=None,
        banner_wait=1.0,
        timeout=1.9
    ))]
    fn new(
        probe_command: Option<String>,
        match_prefix: Option<String>,
        banner_wait: f64,
        timeout: f64,
    ) -> Self {
        Self {
            probe_command,
            match_prefix: match_prefix.unwrap_or_default(),
            banner_wait,
            timeout,
        }
    }

    #[getter]
    pub(crate) fn probe_command(&self) -> Option<String> {
        self.probe_command.clone()
    }

    #[getter]
    pub(crate) fn match_prefix(&self) -> String {
        self.match_prefix.clone()
    }

    #[getter]
    pub(crate) fn banner_wait(&self) -> f64 {
        self.banner_wait
    }

    #[getter]
    pub(crate) fn timeout(&self) -> f64 {
        self.timeout
    }

    #[gen_stub(override_return_type(type_repr = "typing.Awaitable[typing.Any]", imports = ("typing",)))]
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        host: String,
        port: u16,
    ) -> PyResult<Bound<'py, PyAny>> {
        let config = self.clone();
        spawn_future(py, async move {
            let timeout = Duration::from_secs_f64(config.timeout.max(0.01));
            let banner_wait =
                Duration::from_secs_f64(config.banner_wait.max(0.0))
                    .min(timeout);
            let response_wait = timeout
                .saturating_sub(banner_wait)
                .max(Duration::from_millis(100));
            let matched = probe_tcp(
                &host,
                port,
                &TcpProbe {
                    connect_timeout: timeout,
                    banner_wait,
                    response_wait,
                    command: config.probe_command.as_deref(),
                    match_prefix: &config.match_prefix,
                },
            )
            .await
            .is_some();
            Python::attach(|py| {
                if matched {
                    Ok(PyBool::new(py, true).to_owned().into_any().unbind())
                } else {
                    Ok(py.None())
                }
            })
        })
    }
}

pub fn register(discovery_mod: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = discovery_mod.py();
    let module = PyModule::new(py, "fingerprints")?;
    module.setattr(
        "__doc__",
        "Reusable fingerprint probes for generic mDNS service types.",
    )?;
    module.add_class::<TcpFingerprint>()?;
    helpers::attach_submodule(
        discovery_mod,
        &module,
        "raydriver.core.discovery.fingerprints",
    )?;
    Ok(())
}
