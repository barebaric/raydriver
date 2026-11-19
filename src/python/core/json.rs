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

/// Converts a Python value into its serde JSON equivalent; non-JSON
/// types raise.
pub(crate) fn py_to_value(value: &Bound<'_, PyAny>) -> PyResult<Value> {
    if value.is_none() {
        Ok(Value::Null)
    } else if let Ok(flag) = value.extract::<bool>() {
        Ok(Value::Bool(flag))
    } else if let Ok(int) = value.extract::<i64>() {
        Ok(Value::from(int))
    } else if let Ok(float) = value.extract::<f64>() {
        Ok(Value::from(float))
    } else if let Ok(text) = value.extract::<String>() {
        Ok(Value::String(text))
    } else if let Ok(list) = value.cast::<PyList>() {
        let items: PyResult<Vec<Value>> =
            list.iter().map(|item| py_to_value(&item)).collect();
        Ok(Value::Array(items?))
    } else if let Ok(dict) = value.cast::<PyDict>() {
        let mut entries = serde_json::Map::new();
        for (key, item) in dict.iter() {
            let key: String = key.extract()?;
            entries.insert(key, py_to_value(&item)?);
        }
        Ok(Value::Object(entries))
    } else {
        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            "value is not JSON-serializable",
        ))
    }
}
