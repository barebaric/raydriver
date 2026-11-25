//! `raydriver.driver.DriverInfo`: typed driver metadata.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::core::driver::DriverInfo as CoreInfo;
use crate::python::core::json::value_to_py;

/// Typed identity and capabilities of one driver class.
#[gen_stub_pyclass]
#[pyclass(frozen, module = "raydriver.driver")]
pub struct DriverInfo {
    inner: CoreInfo,
}

impl DriverInfo {
    pub(crate) fn new(inner: CoreInfo) -> Self {
        Self { inner }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl DriverInfo {
    /// The class name, which is also the persisted driver name.
    #[getter]
    fn name(&self) -> String {
        self.inner.name.to_string()
    }

    /// The physical connection the driver dials.
    #[getter]
    fn connection(&self) -> String {
        self.inner.connection.as_str().to_string()
    }

    /// The maturity level (`stable`, `untested`, ...).
    #[getter]
    fn maturity(&self) -> String {
        self.inner.maturity.as_str().to_string()
    }

    /// Whether the protocol speaks G-code.
    #[getter]
    fn uses_gcode(&self) -> bool {
        self.inner.uses_gcode
    }

    /// The WCS id that denotes machine space.
    #[getter]
    fn machine_space_wcs(&self) -> String {
        self.inner.machine_space_wcs.to_string()
    }

    /// The selectable work-coordinate systems.
    #[getter]
    fn supported_wcs(&self) -> Vec<String> {
        self.inner
            .supported_wcs
            .iter()
            .map(|wcs| wcs.to_string())
            .collect()
    }

    /// The connection config keys the driver accepts.
    #[getter]
    fn config_keys(&self) -> Vec<String> {
        self.inner
            .config_keys
            .iter()
            .map(|key| key.to_string())
            .collect()
    }

    /// The capability flags.
    #[getter]
    #[gen_stub(override_return_type(
        type_repr = "dict[builtins.str, builtins.bool]"
    ))]
    fn capabilities<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        Ok(value_to_py(py, &self.inner.capabilities_json())?.into_bound(py))
    }

    /// The wire form served by `driver/list`.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = value_to_py(py, &self.inner.to_json())?.into_bound(py);
        Ok(dict.cast_into::<PyDict>()?)
    }
}
