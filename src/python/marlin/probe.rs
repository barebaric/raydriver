//! The `raydriver.marlin.probe` submodule: the device census.

use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

use crate::python::core::json::value_to_py;

/// Builds the census dict from the raw Marlin census-command
/// responses.
///
/// Pure data transformation with no I/O; the caller is responsible
/// for communicating with the device and passing the collected
/// response lines. `boot_lines` are startup messages observed
/// before the handshake; a version found there is more specific
/// than the `FIRMWARE_NAME` token M115 reports.
#[gen_stub_pyfunction(module = "raydriver.marlin.probe")]
#[pyfunction]
#[pyo3(signature = (m115_lines, m211_lines, m503_lines, m149_lines,
                    boot_lines=None))]
fn describe<'py>(
    py: Python<'py>,
    m115_lines: Vec<String>,
    m211_lines: Vec<String>,
    m503_lines: Vec<String>,
    m149_lines: Vec<String>,
    boot_lines: Option<Vec<String>>,
) -> PyResult<Bound<'py, PyAny>> {
    let census = crate::marlin::probe::describe(
        &m115_lines,
        &m211_lines,
        &m503_lines,
        &m149_lines,
        &boot_lines.unwrap_or_default(),
    );
    let dict = value_to_py(py, &census)?;
    Ok(dict.into_bound(py))
}

pub fn register(marlin_mod: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = marlin_mod.py();
    let module = PyModule::new(py, "probe")?;
    module.setattr("__doc__", "Device census for Marlin controllers.")?;
    module.add_function(wrap_pyfunction!(describe, &module)?)?;
    crate::python::core::discovery::helpers::attach_submodule(
        marlin_mod,
        &module,
        "raydriver.marlin.probe",
    )?;
    Ok(())
}
