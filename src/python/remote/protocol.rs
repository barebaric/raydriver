//! Python bindings for the remote wire codec.

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3_stub_gen::derive::gen_stub_pyfunction;
use serde_json::Value;

use crate::core::remote::protocol as core;
use crate::python::core::json::value_to_py;
use crate::python::core::types::DeviceState as PyDeviceState;
use crate::python::events::SessionEvent;
use crate::python::remote::bridge::py_to_json;
use crate::python::remote::events::{event_to_wire, wire_to_py_event};

fn protocol_error(py: Python<'_>, err: &core::ProtocolError) -> PyErr {
    let result: PyResult<PyErr> = (|| {
        let module = py.import("raydriver.remote.protocol")?;
        let cls = module.getattr("ProtocolError")?;
        let value = cls.call1((err.code.clone(), err.message.clone()))?;
        Ok(PyErr::from_value(value))
    })();
    result.unwrap_or_else(|_| {
        pyo3::exceptions::PyValueError::new_err(err.to_string())
    })
}

#[gen_stub_pyfunction(module = "raydriver.remote")]
#[pyfunction]
#[pyo3(signature = (message, max_bytes = core::MAX_FRAME_BYTES))]
pub(crate) fn encode_frame(
    py: Python<'_>,
    message: Py<PyAny>,
    max_bytes: usize,
) -> PyResult<Py<PyBytes>> {
    let value = py_to_json(message.bind(py))?;
    let payload = serde_json::to_vec(&value).map_err(|err| {
        protocol_error(
            py,
            &core::ProtocolError::new(core::BAD_REQUEST, err.to_string()),
        )
    })?;
    if payload.len() > max_bytes {
        return Err(protocol_error(
            py,
            &core::ProtocolError::new(
                core::FRAME_TOO_LARGE,
                format!(
                    "frame of {} bytes exceeds the {max_bytes}-byte limit",
                    payload.len()
                ),
            ),
        ));
    }
    let mut frame = (payload.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(&payload);
    Ok(PyBytes::new(py, &frame).unbind())
}

#[gen_stub_pyfunction(module = "raydriver.remote")]
#[pyfunction]
#[pyo3(signature = (payload, max_bytes = core::MAX_FRAME_BYTES))]
pub(crate) fn decode_frame(
    py: Python<'_>,
    payload: Vec<u8>,
    max_bytes: usize,
) -> PyResult<Py<PyAny>> {
    if payload.len() > max_bytes {
        return Err(protocol_error(
            py,
            &core::ProtocolError::new(
                core::FRAME_TOO_LARGE,
                format!(
                    "frame of {} bytes exceeds the {max_bytes}-byte limit",
                    payload.len()
                ),
            ),
        ));
    }
    let value: Value = serde_json::from_slice(&payload).map_err(|_| {
        protocol_error(
            py,
            &core::ProtocolError::new(
                core::BAD_REQUEST,
                "frame is not valid UTF-8 JSON",
            ),
        )
    })?;
    if !value.is_object() {
        return Err(protocol_error(
            py,
            &core::ProtocolError::new(
                core::BAD_REQUEST,
                "frame must be a JSON object",
            ),
        ));
    }
    value_to_py(py, &value)
}

#[gen_stub_pyfunction(module = "raydriver.remote")]
#[pyfunction]
#[pyo3(signature = (msg_id, type_, payload = None))]
pub(crate) fn request(
    py: Python<'_>,
    msg_id: i64,
    type_: &str,
    payload: Option<Py<PyAny>>,
) -> PyResult<Py<PyAny>> {
    let payload = match payload {
        Some(payload) => py_to_json(payload.bind(py))?,
        None => Value::Object(Default::default()),
    };
    value_to_py(py, &core::request(msg_id, type_, payload))
}

#[gen_stub_pyfunction(module = "raydriver.remote")]
#[pyfunction]
#[pyo3(signature = (msg_id, payload = None))]
pub(crate) fn response(
    py: Python<'_>,
    msg_id: i64,
    payload: Option<Py<PyAny>>,
) -> PyResult<Py<PyAny>> {
    let payload = match payload {
        Some(payload) => py_to_json(payload.bind(py))?,
        None => Value::Object(Default::default()),
    };
    value_to_py(py, &core::response(msg_id, payload))
}

#[gen_stub_pyfunction(module = "raydriver.remote")]
#[pyfunction]
pub(crate) fn error_response(
    py: Python<'_>,
    msg_id: i64,
    code: &str,
    message: &str,
) -> PyResult<Py<PyAny>> {
    value_to_py(py, &core::error_response(msg_id, code, message))
}

#[gen_stub_pyfunction(module = "raydriver.remote")]
#[pyfunction]
pub(crate) fn event_frame(
    py: Python<'_>,
    session_id: &str,
    seq: i64,
    event: Py<PyAny>,
) -> PyResult<Py<PyAny>> {
    let event = py_to_json(event.bind(py))?;
    value_to_py(py, &core::event_frame(session_id, seq, event))
}

#[gen_stub_pyfunction(module = "raydriver.remote")]
#[pyfunction]
pub(crate) fn welcome_payload(
    py: Python<'_>,
    raydriver_version: &str,
    auth_required: bool,
) -> PyResult<Py<PyAny>> {
    value_to_py(py, &core::welcome_payload(raydriver_version, auth_required))
}

#[gen_stub_pyfunction(module = "raydriver.remote")]
#[pyfunction]
pub(crate) fn encode_state(
    py: Python<'_>,
    state: PyRef<'_, PyDeviceState>,
) -> PyResult<Py<PyAny>> {
    value_to_py(py, &core::encode_state(&state.inner))
}

#[gen_stub_pyfunction(module = "raydriver.remote")]
#[pyfunction]
pub(crate) fn decode_state(
    py: Python<'_>,
    payload: Py<PyAny>,
) -> PyResult<PyDeviceState> {
    let value = py_to_json(payload.bind(py))?;
    let state =
        core::decode_state(&value).map_err(|err| protocol_error(py, &err))?;
    Ok(PyDeviceState::new(state))
}

#[gen_stub_pyfunction(module = "raydriver.remote")]
#[pyfunction]
pub(crate) fn encode_event(
    py: Python<'_>,
    event: PyRef<'_, SessionEvent>,
) -> PyResult<Py<PyAny>> {
    value_to_py(py, &event_to_wire(&event).to_json())
}

#[gen_stub_pyfunction(module = "raydriver.remote")]
#[pyfunction]
pub(crate) fn decode_event(
    py: Python<'_>,
    payload: Py<PyAny>,
) -> PyResult<Option<Py<SessionEvent>>> {
    let value = py_to_json(payload.bind(py))?;
    let Some(event) = core::WireEvent::from_json(&value) else {
        return Ok(None);
    };
    let Some(event) = wire_to_py_event(py, &event)? else {
        return Ok(None);
    };
    Ok(Some(event.extract(py)?))
}
