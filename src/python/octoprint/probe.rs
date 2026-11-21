//! The `raydriver.octoprint.probe` submodule: the device census.

use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

use crate::python::core::json::{py_to_value, value_to_py};

/// Builds the census dict from the parsed OctoPrint API payloads.
///
/// Pure data transformation with no I/O; the caller is responsible
/// for the HTTP requests. *version_info* is the `GET /api/version`
/// response, *printer_info* the `GET /api/printer` response
/// (either may be `None` when the request failed).
#[gen_stub_pyfunction(module = "raydriver.octoprint.probe")]
#[pyfunction]
#[pyo3(signature = (version_info=None, printer_info=None))]
fn describe<'py>(
    py: Python<'py>,
    version_info: Option<&Bound<'_, PyAny>>,
    printer_info: Option<&Bound<'_, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let version = match version_info {
        Some(value) if !value.is_none() => py_to_value(value)?,
        _ => serde_json::Value::Null,
    };
    let printer = match printer_info {
        Some(value) if !value.is_none() => py_to_value(value)?,
        _ => serde_json::Value::Null,
    };
    let census = crate::octoprint::probe::describe(&version, &printer);
    let dict = value_to_py(py, &census)?;
    Ok(dict.into_bound(py))
}

pub fn register(octoprint_mod: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = octoprint_mod.py();
    let module = PyModule::new(py, "probe")?;
    module.setattr("__doc__", "Device census for OctoPrint servers.")?;
    module.add_function(wrap_pyfunction!(describe, &module)?)?;
    crate::python::core::discovery::helpers::attach_submodule(
        octoprint_mod,
        &module,
        "raydriver.octoprint.probe",
    )?;
    Ok(())
}
