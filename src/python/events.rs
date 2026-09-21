//! Python-facing session events.
//!
//! Implements [`SessionEvents`] by marshaling each event onto the
//! caller's asyncio loop via `call_soon_threadsafe`, invoking
//! `event_callback(name, payload)` on the loop thread.  The loop
//! reference is captured at `connect()` time (an async call, hence
//! on the loop thread).

use std::collections::HashMap;
use std::sync::Mutex;

use pyo3::prelude::*;

use crate::grbl::session::SessionEvents;
use crate::grbl::types::{DeviceState, TransportStatus};

pub(crate) struct PyEvents {
    event_callback: Option<Py<PyAny>>,
    loop_: Mutex<Option<Py<PyAny>>>,
    progress_callback: Mutex<Option<Py<PyAny>>>,
}

impl PyEvents {
    pub(crate) fn new(event_callback: Option<Py<PyAny>>) -> Self {
        Self {
            event_callback,
            loop_: Mutex::new(None),
            progress_callback: Mutex::new(None),
        }
    }

    /// Remember the running event loop so events can be marshaled
    /// from tokio worker threads.
    pub(crate) fn capture_loop(&self, py: Python<'_>) -> PyResult<()> {
        let mut slot = self.loop_.lock().unwrap();
        if slot.is_none() {
            let asyncio = py.import("asyncio")?;
            *slot = Some(asyncio.call_method0("get_running_loop")?.unbind());
        }
        Ok(())
    }

    pub(crate) fn set_progress_callback(&self, cb: Option<Py<PyAny>>) {
        *self.progress_callback.lock().unwrap() = cb;
    }

    fn emit<F>(&self, name: &str, payload: F)
    where
        F: FnOnce(Python<'_>) -> PyResult<Py<PyAny>>,
    {
        if self.event_callback.is_none() {
            return;
        }
        // The GIL must be attached before touching Python heap
        // pointers (Py::clone panics otherwise), so borrows — not
        // clones — are taken inside the attach scope.
        let result: PyResult<()> = Python::attach(|py| {
            let callback = self
                .event_callback
                .as_ref()
                .expect("checked above")
                .bind(py);
            let loop_guard = self.loop_.lock().unwrap();
            let Some(loop_) = loop_guard.as_ref() else {
                return Ok(());
            };
            let payload = payload(py)?;
            loop_.bind(py).call_method1(
                "call_soon_threadsafe",
                (callback, name, payload),
            )?;
            Ok(())
        });
        if let Err(err) = result {
            log::warn!("failed to emit session event {name}: {err}");
        }
    }
}

impl SessionEvents for PyEvents {
    fn state_changed(&self, state: &DeviceState) {
        let state = state.clone();
        self.emit("state_changed", move |py| {
            let obj = Py::new(py, super::types::DeviceState::new(state))?;
            Ok(obj.into_any())
        });
    }

    fn connection_status(
        &self,
        status: TransportStatus,
        message: Option<&str>,
    ) {
        let status = status.name().to_string();
        let message = message.map(str::to_string);
        self.emit("connection_status_changed", move |py| {
            Ok((status.clone(), message.clone())
                .into_pyobject(py)?
                .unbind()
                .into_any())
        });
    }

    fn command_status(&self, status: TransportStatus, message: Option<&str>) {
        let status = status.name().to_string();
        let message = message.map(str::to_string);
        self.emit("command_status_changed", move |py| {
            Ok((status.clone(), message.clone())
                .into_pyobject(py)?
                .unbind()
                .into_any())
        });
    }

    fn job_finished(&self) {
        self.emit("job_finished", move |py| Ok(super::pyfuture::none_obj(py)));
    }

    fn probe_status(&self, message: &str) {
        let message = message.to_string();
        self.emit("probe_status_changed", move |py| {
            Ok(message.clone().into_pyobject(py)?.unbind().into_any())
        });
    }

    fn wcs_updated(&self, offsets: &HashMap<String, (f64, f64, f64)>) {
        let offsets = offsets.clone();
        self.emit("wcs_updated", move |py| {
            Ok(offsets.clone().into_pyobject(py)?.unbind().into_any())
        });
    }

    fn config_changed(&self, key: &str, value: i64) {
        let key = key.to_string();
        self.emit("config_changed", move |py| {
            Ok((key.clone(), value).into_pyobject(py)?.unbind().into_any())
        });
    }

    fn command_done(&self, op_index: i64) {
        if self.progress_callback.lock().unwrap().is_none() {
            return;
        }
        let result: PyResult<()> = Python::attach(|py| {
            let progress_guard = self.progress_callback.lock().unwrap();
            let Some(callback) = progress_guard.as_ref() else {
                return Ok(());
            };
            let loop_guard = self.loop_.lock().unwrap();
            let Some(loop_) = loop_guard.as_ref() else {
                return Ok(());
            };
            loop_
                .bind(py)
                .call_method1("call_soon_threadsafe", (callback, op_index))?;
            Ok(())
        });
        if let Err(err) = result {
            log::warn!("failed to report command_done: {err}");
        }
    }
}
