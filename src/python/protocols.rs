//! Python bindings for the protocol registry (`raydriver.protocols`).
//!
//! Registered as the `raydriver.protocols` submodule (stub-only: no
//! Python file shadows it).

use pyo3::exceptions::PyKeyError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::{
    gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods,
};

use crate::core::remote::registry::ProtocolInfo as CoreProtocolInfo;
use crate::python::core::json::value_to_py;

/// Metadata describing one session protocol.
///
/// ``capabilities`` is derived from the method/property surface so
/// the two never drift apart. ``dialect_family`` is a hint clients
/// can map to their own command-dialect definitions; it is not a
/// driver selection.
#[gen_stub_pyclass(module = "raydriver.protocols")]
#[pyclass(frozen, module = "raydriver.protocols")]
pub struct ProtocolInfo {
    inner: CoreProtocolInfo,
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
#[gen_stub_pyfunction(module = "raydriver.protocols")]
#[pyfunction]
fn protocol_info(name: &str) -> PyResult<ProtocolInfo> {
    crate::protocols::registry()
        .info(name)
        .cloned()
        .map(|inner| ProtocolInfo { inner })
        .ok_or_else(|| PyKeyError::new_err(name.to_owned()))
}

/// Every protocol's metadata, keyed by protocol name.
#[gen_stub_pyfunction(module = "raydriver.protocols")]
#[gen_stub(override_return_type(
    type_repr = "dict[builtins.str, ProtocolInfo]",
))]
#[pyfunction]
fn protocols(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let dict = PyDict::new(py);
    let registry = crate::protocols::registry();
    for name in registry.names() {
        let Some(inner) = registry.info(name).cloned() else {
            continue;
        };
        dict.set_item(name, Py::new(py, ProtocolInfo { inner })?)?;
    }
    Ok(dict)
}

/// The ``protocol/list`` payload: every protocol's metadata.
#[gen_stub_pyfunction(module = "raydriver.protocols")]
#[gen_stub(override_return_type(type_repr = "dict[builtins.str, builtins.object]", imports = ("builtins",)))]
#[pyfunction]
fn protocols_payload(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let payload = crate::protocols::registry().payload();
    Ok(value_to_py(py, &payload)?.into_bound(py))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let module = PyModule::new(py, "protocols")?;
    module.setattr(
        "__doc__",
        "Protocol registry: identity, config shape and capability \
         claims for every hostable session protocol.",
    )?;
    module.add(
        "__all__",
        vec!["PROTOCOLS", "ProtocolInfo", "protocol_info", "protocols"],
    )?;
    // The registry as a name -> metadata mapping.
    let protocols_dict = protocols(py)?;
    module.setattr("PROTOCOLS", protocols_dict)?;
    module.add_class::<ProtocolInfo>()?;
    module.add_function(wrap_pyfunction!(protocol_info, &module)?)?;
    module.add_function(wrap_pyfunction!(protocols, &module)?)?;
    module.add_function(wrap_pyfunction!(protocols_payload, &module)?)?;
    crate::python::core::discovery::helpers::attach_submodule(
        m,
        &module,
        "raydriver.protocols",
    )?;
    Ok(())
}

pyo3_stub_gen::module_variable!(
    "raydriver.protocols",
    "PROTOCOLS",
    std::collections::HashMap<String, ProtocolInfo>
);
