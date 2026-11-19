//! The `raydriver.grbl.probe` submodule: the device census.

use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

use crate::python::core::json::value_to_py;

/// Builds the census dict from the raw Grbl `$I` and `$$` response
/// lines.
///
/// Pure data transformation with no I/O; the caller is responsible
/// for communicating with the device and passing the collected
/// response lines.
#[gen_stub_pyfunction(module = "raydriver.grbl.probe")]
#[pyfunction]
fn describe(
    py: Python<'_>,
    build_info: Vec<String>,
    settings_lines: Vec<String>,
) -> PyResult<Py<PyAny>> {
    let census = crate::grbl::probe::describe(&build_info, &settings_lines);
    value_to_py(py, &census)
}

pub fn register(grbl_mod: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = grbl_mod.py();
    let module = PyModule::new(py, "probe")?;
    module.setattr("__doc__", "Device census for GRBL controllers.")?;
    module.add_function(wrap_pyfunction!(describe, &module)?)?;
    crate::python::core::discovery::helpers::attach_submodule(
        grbl_mod,
        &module,
        "raydriver.grbl.probe",
    )?;
    Ok(())
}
