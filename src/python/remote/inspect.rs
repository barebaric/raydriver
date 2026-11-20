//! Python bindings for read-only daemon inspection.

use pyo3::prelude::*;
use pyo3::types::PyTuple;
use pyo3_stub_gen::derive::gen_stub_pyfunction;
use serde_json::Value;

use crate::core::remote::inspect as core;
use crate::python::core::json::value_to_py;
use crate::python::pyfuture::spawn_future;
use crate::python::remote::bridge::py_to_json;

#[gen_stub_pyfunction(module = "raydriver.remote.inspect")]
#[pyfunction]
pub(crate) fn daemon_rpc<'py>(
    py: Python<'py>,
    host: String,
    port: u16,
    token: Option<String>,
    tls_ca: Option<String>,
    requests: Py<PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let requests: Vec<(String, Value)> = requests
        .bind(py)
        .try_iter()?
        .map(|entry| {
            let entry = entry?;
            let pair = entry.cast::<PyTuple>()?;
            let type_: String = pair.get_item(0)?.extract()?;
            let payload = py_to_json(&pair.get_item(1)?)?;
            Ok((type_, payload))
        })
        .collect::<PyResult<_>>()?;
    spawn_future(py, async move {
        let borrowed: Vec<(&str, Value)> = requests
            .iter()
            .map(|(type_, payload)| (type_.as_str(), payload.clone()))
            .collect();
        let (welcome, replies) = core::daemon_rpc(
            &host,
            port,
            token.as_deref(),
            tls_ca.as_deref(),
            None,
            &borrowed,
        )
        .await
        .map_err(pyo3::exceptions::PyConnectionError::new_err)?;
        Python::attach(|py| {
            let welcome = value_to_py(py, &welcome)?;
            let replies = value_to_py(py, &Value::Array(replies))?;
            Ok(PyTuple::new(py, [welcome, replies])?.unbind().into_any())
        })
    })
}

#[gen_stub_pyfunction(module = "raydriver.remote.inspect")]
#[pyfunction]
pub(crate) fn welcome_probe<'py>(
    py: Python<'py>,
    host: String,
    port: u16,
) -> PyResult<Bound<'py, PyAny>> {
    spawn_future(py, async move {
        let welcome = core::welcome_probe(&host, port)
            .await
            .map_err(pyo3::exceptions::PyConnectionError::new_err)?;
        Python::attach(|py| value_to_py(py, &welcome))
    })
}

#[gen_stub_pyfunction(module = "raydriver.remote.inspect")]
#[pyfunction]
#[pyo3(signature = (host, port, token = None, tls_ca = None))]
pub(crate) fn snapshot<'py>(
    py: Python<'py>,
    host: String,
    port: u16,
    token: Option<String>,
    tls_ca: Option<String>,
) -> PyResult<Bound<'py, PyAny>> {
    spawn_future(py, async move {
        let snapshot =
            core::snapshot(&host, port, token.as_deref(), tls_ca.as_deref())
                .await
                .map_err(pyo3::exceptions::PyConnectionError::new_err)?;
        Python::attach(|py| value_to_py(py, &snapshot))
    })
}
