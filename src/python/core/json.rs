//! serde JSON ⇆ Python value conversion.
//!
//! The Python/Rust boundary passes JSON-serializable data only;
//! session methods that return structured results (device
//! censuses) hand them across as plain dicts.

use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyList, PyString};
use serde_json::Value;

/// Converts a serde JSON value into its Python equivalent.
pub(crate) fn value_to_py(
    py: Python<'_>,
    value: &Value,
) -> PyResult<Py<PyAny>> {
    let any = match value {
        Value::Null => py.None(),
        Value::Bool(flag) => {
            PyBool::new(py, *flag).to_owned().into_any().unbind()
        }
        Value::Number(number) => {
            if let Some(int) = number.as_i64() {
                int.into_pyobject(py)?.unbind().into_any()
            } else {
                PyFloat::new(py, number.as_f64().unwrap_or(0.0))
                    .into_pyobject(py)?
                    .unbind()
                    .into_any()
            }
        }
        Value::String(text) => PyString::new(py, text).unbind().into_any(),
        Value::Array(items) => {
            let converted: PyResult<Vec<Py<PyAny>>> =
                items.iter().map(|item| value_to_py(py, item)).collect();
            PyList::new(py, converted?)?.unbind().into_any()
        }
        Value::Object(entries) => {
            let dict = PyDict::new(py);
            for (key, item) in entries {
                dict.set_item(key, value_to_py(py, item)?)?;
            }
            dict.unbind().into_any()
        }
    };
    Ok(any)
}
