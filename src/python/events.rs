//! Python-facing session events.
//!
//! Implements [`SessionEvents`] by marshaling each event onto the
//! caller's asyncio loop.  Events are queued on the Rust side and a
//! single `call_soon_threadsafe(drain)` signal per batch wakes the
//! loop, which invokes `event_callback(name, payload)` for every
//! queued event on the loop thread.  Batching keeps the loop's
//! self-pipe write rate at one signal per batch regardless of event
//! volume.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pyo3::prelude::*;

use crate::grbl::session::SessionEvents;
use crate::grbl::types::{DeviceState, TransportStatus};

/// One queued event: the event name and its payload object.
type QueuedEvent = (String, Py<PyAny>);

pub(crate) struct PyEvents {
    event_callback: Option<Py<PyAny>>,
    loop_: Mutex<Option<Py<PyAny>>>,
    progress_callback: Mutex<Option<Py<PyAny>>>,
    queue: Arc<EventQueue>,
}

/// Events waiting to be delivered on the loop thread, plus a flag
/// telling whether a drain call is already scheduled on the loop.
#[derive(Default)]
struct EventQueue {
    pending: Mutex<Vec<QueuedEvent>>,
    scheduled: AtomicBool,
}

impl EventQueue {
    /// Queue an event and return true when the caller must schedule
    /// a drain (only the caller that flips the flag schedules).
    fn push(&self, event: QueuedEvent) -> bool {
        let mut pending = self.pending.lock().unwrap();
        pending.push(event);
        !self.scheduled.swap(true, Ordering::SeqCst)
    }

    /// Undo a `push` that could not schedule a drain, so a later
    /// event retries instead of the queue wedging forever.
    fn unschedule(&self) {
        self.scheduled.store(false, Ordering::SeqCst);
    }

    /// Take all queued events and clear the scheduled flag.
    fn take(&self) -> Vec<QueuedEvent> {
        let mut pending = self.pending.lock().unwrap();
        self.scheduled.store(false, Ordering::SeqCst);
        std::mem::take(&mut *pending)
    }
}

/// The loop-side drain callable: invoked via
/// `loop.call_soon_threadsafe(drain)`; runs on the event loop
/// thread and delivers every queued event.
#[pyclass(module = "raydriver.grbl")]
struct EventDrainer {
    queue: Arc<EventQueue>,
    callback: Py<PyAny>,
}

#[pymethods]
impl EventDrainer {
    fn __call__(&self, py: Python<'_>) {
        self.take_and_deliver(py);
    }
}

impl PyEvents {
    pub(crate) fn new(event_callback: Option<Py<PyAny>>) -> Self {
        Self {
            event_callback,
            loop_: Mutex::new(None),
            progress_callback: Mutex::new(None),
            queue: Arc::new(EventQueue::default()),
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

    /// Queue `(name, payload)` for delivery and schedule the loop
    /// drain if this caller flipped the scheduled flag.  The GIL is
    /// attached only for payload construction and the single
    /// `call_soon_threadsafe` signal.
    fn schedule<F>(&self, name: &str, payload: F)
    where
        F: FnOnce(Python<'_>) -> PyResult<Py<PyAny>>,
    {
        if self.event_callback.is_none() {
            return;
        }
        let result: PyResult<()> = Python::attach(|py| {
            let payload = payload(py)?;
            if self.queue.push((name.to_string(), payload)) {
                let loop_guard = self.loop_.lock().unwrap();
                let Some(loop_) = loop_guard.as_ref() else {
                    self.queue.unschedule();
                    return Ok(());
                };
                let loop_py = loop_.clone_ref(py);
                drop(loop_guard);
                let callback = self
                    .event_callback
                    .as_ref()
                    .expect("checked above")
                    .bind(py);
                let drainer = Py::new(
                    py,
                    EventDrainer {
                        queue: self.queue.clone(),
                        callback: callback.clone().unbind(),
                    },
                )?;
                loop_py
                    .bind(py)
                    .call_method1("call_soon_threadsafe", (drainer,))?;
            }
            Ok(())
        });
        if let Err(err) = result {
            self.queue.unschedule();
            log::warn!("failed to schedule session event {name}: {err}");
        }
    }

    /// Queue a progress report for delivery.
    fn schedule_progress(&self, op_index: i64) {
        if self.progress_callback.lock().unwrap().is_none() {
            return;
        }
        let result: PyResult<()> = Python::attach(|py| {
            let progress_guard = self.progress_callback.lock().unwrap();
            let Some(callback) = progress_guard.as_ref() else {
                return Ok(());
            };
            let callback = callback.clone_ref(py);
            drop(progress_guard);
            let loop_guard = self.loop_.lock().unwrap();
            let Some(loop_) = loop_guard.as_ref() else {
                return Ok(());
            };
            let loop_py = loop_.clone_ref(py);
            drop(loop_guard);
            let payload = {
                let obj = op_index.into_pyobject(py)?;
                obj.unbind().into_any()
            };
            if self.queue.push(("__progress__".to_string(), payload)) {
                let drainer = Py::new(
                    py,
                    EventDrainer {
                        queue: self.queue.clone(),
                        callback,
                    },
                )?;
                if let Err(err) = loop_py
                    .bind(py)
                    .call_method1("call_soon_threadsafe", (drainer,))
                {
                    self.queue.unschedule();
                    return Err(err);
                }
            }
            Ok(())
        });
        if let Err(err) = result {
            self.queue.unschedule();
            log::warn!("failed to schedule command_done: {err}");
        }
    }

    /// Deliver one queued progress report (called on the loop
    /// thread by the drainer).
    fn deliver_progress(
        callback: &Py<PyAny>,
        py: Python<'_>,
        payload: &Py<PyAny>,
    ) {
        if let Err(err) = callback.call1(py, (payload,)) {
            log::warn!("progress callback failed: {err}");
        }
    }
}

impl SessionEvents for PyEvents {
    fn state_changed(&self, state: &DeviceState) {
        let state = state.clone();
        self.schedule("state_changed", move |py| {
            let obj = Py::new(
                py,
                crate::python::grbl::types::DeviceState::new(state),
            )?;
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
        self.schedule("connection_status_changed", move |py| {
            Ok((status.clone(), message.clone())
                .into_pyobject(py)?
                .unbind()
                .into_any())
        });
    }

    fn command_status(&self, status: TransportStatus, message: Option<&str>) {
        let status = status.name().to_string();
        let message = message.map(str::to_string);
        self.schedule("command_status_changed", move |py| {
            Ok((status.clone(), message.clone())
                .into_pyobject(py)?
                .unbind()
                .into_any())
        });
    }

    fn job_finished(&self) {
        self.schedule("job_finished", move |py| {
            Ok(super::pyfuture::none_obj(py))
        });
    }

    fn probe_status(&self, message: &str) {
        let message = message.to_string();
        self.schedule("probe_status_changed", move |py| {
            Ok(message.clone().into_pyobject(py)?.unbind().into_any())
        });
    }

    fn wcs_updated(&self, offsets: &HashMap<String, (f64, f64, f64)>) {
        let offsets = offsets.clone();
        self.schedule("wcs_updated", move |py| {
            Ok(offsets.clone().into_pyobject(py)?.unbind().into_any())
        });
    }

    fn config_changed(&self, key: &str, value: i64) {
        let key = key.to_string();
        self.schedule("config_changed", move |py| {
            Ok((key.clone(), value).into_pyobject(py)?.unbind().into_any())
        });
    }

    fn command_done(&self, op_index: i64) {
        self.schedule_progress(op_index);
    }
}

impl EventDrainer {
    /// Progress reports use a sentinel event name and route to the
    /// progress callback instead of the event callback.
    fn take_and_deliver(&self, py: Python<'_>) {
        for (name, payload) in self.queue.take() {
            if name == "__progress__" {
                PyEvents::deliver_progress(&self.callback, py, &payload);
            } else {
                let callback = self.callback.bind(py);
                if let Err(err) = callback.call1((name, payload)) {
                    log::warn!("event callback failed: {err}");
                }
            }
        }
    }
}
