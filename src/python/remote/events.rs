//! Conversion between Python session events and wire events.

use pyo3::prelude::*;

use crate::core::remote::protocol::WireEvent;
use crate::python::core::types::DeviceState as PyDeviceState;
use crate::python::events::SessionEvent;

pub(crate) fn event_to_wire(event: &SessionEvent) -> WireEvent {
    match event {
        SessionEvent::StateChanged { state } => {
            WireEvent::State(state.inner.clone())
        }
        SessionEvent::ConnectionStatusChanged { status, message } => {
            WireEvent::ConnectionStatus {
                status: status.clone(),
                message: message.clone(),
            }
        }
        SessionEvent::CommandStatusChanged { status, message } => {
            WireEvent::CommandStatus {
                status: status.clone(),
                message: message.clone(),
            }
        }
        SessionEvent::JobFinished {} => WireEvent::JobFinished,
        SessionEvent::ProbeStatusChanged { message } => {
            WireEvent::ProbeStatus {
                message: message.clone(),
            }
        }
        SessionEvent::WcsUpdated { offsets } => WireEvent::WcsUpdated {
            offsets: offsets.clone(),
        },
        SessionEvent::ConfigChanged { key, value } => {
            WireEvent::ConfigChanged {
                key: key.clone(),
                value: *value,
            }
        }
        SessionEvent::RawRx { line } => WireEvent::RawRx { line: line.clone() },
        SessionEvent::RawTx { line } => WireEvent::RawTx { line: line.clone() },
    }
}

/// Extract a Python `SessionEvent` from a callback argument.
pub(crate) fn extract_event(
    py: Python<'_>,
    value: &Py<PyAny>,
) -> Option<SessionEvent> {
    value
        .bind(py)
        .extract::<PyRef<SessionEvent>>()
        .ok()
        .map(|event| match &*event {
            SessionEvent::StateChanged { state } => {
                SessionEvent::StateChanged {
                    state: state.clone(),
                }
            }
            SessionEvent::ConnectionStatusChanged { status, message } => {
                SessionEvent::ConnectionStatusChanged {
                    status: status.clone(),
                    message: message.clone(),
                }
            }
            SessionEvent::CommandStatusChanged { status, message } => {
                SessionEvent::CommandStatusChanged {
                    status: status.clone(),
                    message: message.clone(),
                }
            }
            SessionEvent::JobFinished {} => SessionEvent::JobFinished {},
            SessionEvent::ProbeStatusChanged { message } => {
                SessionEvent::ProbeStatusChanged {
                    message: message.clone(),
                }
            }
            SessionEvent::WcsUpdated { offsets } => SessionEvent::WcsUpdated {
                offsets: offsets.clone(),
            },
            SessionEvent::ConfigChanged { key, value } => {
                SessionEvent::ConfigChanged {
                    key: key.clone(),
                    value: *value,
                }
            }
            SessionEvent::RawRx { line } => {
                SessionEvent::RawRx { line: line.clone() }
            }
            SessionEvent::RawTx { line } => {
                SessionEvent::RawTx { line: line.clone() }
            }
        })
}

/// Builds the Python `SessionEvent` variant object for *event*.
pub(crate) fn wire_to_py_event(
    py: Python<'_>,
    event: &WireEvent,
) -> PyResult<Option<Py<PyAny>>> {
    let base = py.import("raydriver.core")?.getattr("SessionEvent")?;
    match event {
        WireEvent::State(state) => {
            let cls = base.getattr("StateChanged")?;
            let state = Py::new(py, PyDeviceState::new(state.clone()))?;
            Ok(Some(cls.call1((state,))?.unbind()))
        }
        WireEvent::ConnectionStatus { status, message } => {
            let cls = base.getattr("ConnectionStatusChanged")?;
            Ok(Some(cls.call1((status.clone(), message.clone()))?.unbind()))
        }
        WireEvent::CommandStatus { status, message } => {
            let cls = base.getattr("CommandStatusChanged")?;
            Ok(Some(cls.call1((status.clone(), message.clone()))?.unbind()))
        }
        WireEvent::JobFinished => {
            let cls = base.getattr("JobFinished")?;
            Ok(Some(cls.call0()?.unbind()))
        }
        WireEvent::ProbeStatus { message } => {
            let cls = base.getattr("ProbeStatusChanged")?;
            Ok(Some(cls.call1((message.clone(),))?.unbind()))
        }
        WireEvent::WcsUpdated { offsets } => {
            let cls = base.getattr("WcsUpdated")?;
            let dict = pyo3::types::PyDict::new(py);
            for (key, (x, y, z)) in offsets {
                dict.set_item(key, (x, y, z))?;
            }
            Ok(Some(cls.call1((dict,))?.unbind()))
        }
        WireEvent::ConfigChanged { key, value } => {
            let cls = base.getattr("ConfigChanged")?;
            Ok(Some(cls.call1((key.clone(), *value))?.unbind()))
        }
        WireEvent::RawRx { line } => {
            let cls = base.getattr("RawRx")?;
            Ok(Some(cls.call1((line.clone(),))?.unbind()))
        }
        WireEvent::RawTx { line } => {
            let cls = base.getattr("RawTx")?;
            Ok(Some(cls.call1((line.clone(),))?.unbind()))
        }
        _ => Ok(None),
    }
}
