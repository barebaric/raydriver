//! Shared conversion from typed Python config dataclasses to the
//! mapping form consumed by the session bindings.

use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Accept either a plain mapping or a `raydriver.config.SessionConfig`
/// dataclass (anything exposing `to_dict()`), and return the mapping.
pub(crate) fn config_dict<'py>(
    config: Option<&Bound<'py, PyAny>>,
) -> PyResult<Option<Bound<'py, PyDict>>> {
    let Some(config) = config else {
        return Ok(None);
    };
    if let Ok(dict) = config.cast::<PyDict>() {
        return Ok(Some(dict.clone()));
    }
    let dict = config.call_method0("to_dict")?;
    Ok(Some(dict.cast_into::<PyDict>()?))
}
