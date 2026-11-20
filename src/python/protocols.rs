//! Python bindings for the protocol registry (`raydriver.protocols`).
//!
//! Registered on the root module; the pure-Python
//! `raydriver.protocols` package delegates here via module-level
//! `__getattr__`.

use pyo3::exceptions::PyKeyError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::{
    gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods,
};

use crate::core::remote::registry as core;
use crate::python::core::json::value_to_py;

/// Metadata describing one session protocol.
///
/// ``capabilities`` is derived from the method/property surface so
/// the two never drift apart. ``dialect_family`` is a hint clients
/// can map to their own command-dialect definitions; it is not a
/// driver selection.
#[gen_stub_pyclass]
#[pyclass(frozen, module = "raydriver")]
pub struct ProtocolInfo {
    inner: core::ProtocolInfo,
}

#[gen_stub_pymethods]
#[pymethods]
impl ProtocolInfo {
    #[getter]
    fn name(&self) -> String {
        self.inner.name.to_string()
    }

    #[getter]
    fn label(&self) -> String {
        self.inner.label.to_string()
    }

    #[getter]
    fn dialect_family(&self) -> String {
        self.inner.dialect_family.to_string()
    }

    #[getter]
    fn connection(&self) -> String {
        self.inner.connection.to_string()
    }

    #[getter]
    fn config_keys(&self) -> Vec<String> {
        self.inner
            .config_keys
            .iter()
            .map(|k| k.to_string())
            .collect()
    }

    #[getter]
    fn methods(&self) -> Vec<String> {
        self.inner.methods.iter().map(|m| m.to_string()).collect()
    }

    #[getter]
    fn properties(&self) -> Vec<String> {
        self.inner
            .properties
            .iter()
            .map(|p| p.to_string())
            .collect()
    }

    #[getter]
    #[gen_stub(override_return_type(
        type_repr = "dict[builtins.str, builtins.bool]"
    ))]
    fn capabilities<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        Ok(value_to_py(py, &self.inner.capabilities())?.into_bound(py))
    }

    /// The wire form served by ``protocol/list``.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = value_to_py(py, &self.inner.to_json())?.into_bound(py);
        Ok(dict.cast_into::<PyDict>()?)
    }
}

/// Look up a protocol's metadata, raising ``KeyError`` if unknown.
#[gen_stub_pyfunction]
#[pyfunction]
fn protocol_info(name: &str) -> PyResult<ProtocolInfo> {
    core::protocol_info(name)
        .map(|inner| ProtocolInfo { inner })
        .ok_or_else(|| PyKeyError::new_err(name.to_owned()))
}

/// Every protocol's metadata, keyed by protocol name.
#[gen_stub_pyfunction]
#[gen_stub(override_return_type(
    type_repr = "dict[builtins.str, ProtocolInfo]",
))]
#[pyfunction]
fn protocols(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let dict = PyDict::new(py);
    for name in core::names() {
        let Some(inner) = core::protocol_info(name) else {
            continue;
        };
        dict.set_item(name, Py::new(py, ProtocolInfo { inner })?)?;
    }
    Ok(dict)
}

/// The ``protocol/list`` payload: every protocol's metadata.
#[gen_stub_pyfunction]
#[gen_stub(override_return_type(type_repr = "dict[builtins.str, typing.Any]", imports = ("typing",)))]
#[pyfunction]
fn protocols_payload(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    Ok(value_to_py(py, &core::protocols_payload())?.into_bound(py))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ProtocolInfo>()?;
    m.add_function(wrap_pyfunction!(protocol_info, m)?)?;
    m.add_function(wrap_pyfunction!(protocols, m)?)?;
    m.add_function(wrap_pyfunction!(protocols_payload, m)?)?;
    Ok(())
}
