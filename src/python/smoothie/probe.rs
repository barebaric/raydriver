//! The `raydriver.smoothie.probe` submodule: the device census.

use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

use crate::python::core::json::value_to_py;

/// Builds the census dict from raw Smoothieware census-command
/// responses.
///
/// Pure data transformation with no I/O; the caller is responsible
/// for communicating with the device and passing the collected
/// response lines. ``ok`` acknowledgements and ``<...>`` status
/// reports are dropped, and non-numeric ``config-get`` replies
/// yield ``None`` fields.
#[gen_stub_pyfunction(module = "raydriver.smoothie.probe")]
#[pyfunction]
#[pyo3(signature = (version_lines, alpha_max_lines, beta_max_lines,
                    alpha_max_rate_lines, beta_max_rate_lines,
                    acceleration_lines))]
fn describe(
    py: Python<'_>,
    version_lines: Vec<String>,
    alpha_max_lines: Vec<String>,
    beta_max_lines: Vec<String>,
    alpha_max_rate_lines: Vec<String>,
    beta_max_rate_lines: Vec<String>,
    acceleration_lines: Vec<String>,
) -> PyResult<Py<PyAny>> {
    let census = crate::smoothie::probe::describe(
        &version_lines,
        &alpha_max_lines,
        &beta_max_lines,
        &alpha_max_rate_lines,
        &beta_max_rate_lines,
        &acceleration_lines,
    );
    value_to_py(py, &census)
}

pub fn register(smoothie_mod: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = smoothie_mod.py();
    let module = PyModule::new(py, "probe")?;
    module.setattr("__doc__", "Device census for Smoothieware controllers.")?;
    module.add_function(wrap_pyfunction!(describe, &module)?)?;
    crate::python::core::discovery::helpers::attach_submodule(
        smoothie_mod,
        &module,
        "raydriver.smoothie.probe",
    )?;
    Ok(())
}
