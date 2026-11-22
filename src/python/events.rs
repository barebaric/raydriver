//! Python-facing session events.
//!
//! Implements [`SessionEvents`] by marshaling each event onto the
//! caller's asyncio loop.  Events are queued on the Rust side and a
//! single `call_soon_threadsafe(drain)` signal per batch wakes the
//! loop, which materializes one [`SessionEvent`] per queued event and
//! invokes `event_callback(event)` for it on the loop thread.
//! Batching keeps the loop's self-pipe write rate at one signal per
//! batch regardless of event volume.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyclass_complex_enum;

use crate::core::session::SessionEvents;
use crate::core::types::{DeviceState as CoreDeviceState, TransportStatus};

use crate::python::core::types::DeviceState as PyDeviceState;

/// One typed session event, delivered to `event_callback`.
#[gen_stub_pyclass_complex_enum]
#[pyclass(module = "raydriver.core", skip_from_py_object)]
#[derive(Clone)]
pub enum SessionEvent {
    StateChanged {
        state: PyDeviceState,
    },
    ConnectionStatusChanged {
        status: String,
        message: Option<String>,
    },
    CommandStatusChanged {
        status: String,
        message: Option<String>,
    },
    JobFinished {},
    JobError {
        message: String,
    },
    ProbeStatusChanged {
        message: String,
    },
    WcsUpdated {
        offsets: HashMap<String, (f64, f64, f64)>,
    },
    ConfigChanged {
        key: String,
        value: i64,
    },
    RawRx {
        line: String,
    },
    RawTx {
        line: String,
    },
}

/// One queued event: session events go to the event callback, a
/// progress report goes to the progress callback.
enum QueuedEvent {
    Session(SessionEvent),
    Progress(i64),
}

pub(crate) struct PyEvents {
    event_callback: Option<Py<PyAny>>,
    loop_: Mutex<Option<Py<PyAny>>>,
    progress_callback: Arc<Mutex<Option<Py<PyAny>>>>,
    queue: Arc<EventQueue>,
    closed: AtomicBool,
    schedule_lock: Mutex<()>,
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
/// thread and delivers every queued event.  Carries the event
/// callback plus a handle to the shared progress callback slot, so
/// each queued event routes to its current callback no matter which
/// schedule call drained the batch.
#[pyclass(module = "raydriver.core")]
struct EventDrainer {
    queue: Arc<EventQueue>,
    events: Option<Py<PyAny>>,
    progress: Arc<Mutex<Option<Py<PyAny>>>>,
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
            progress_callback: Arc::new(Mutex::new(None)),
            queue: Arc::new(EventQueue::default()),
            closed: AtomicBool::new(false),
            schedule_lock: Mutex::new(()),
        }
    }

    /// Stop accepting new events.  Safe to call while holding the
    /// GIL: it never blocks on an in-flight schedule.
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    /// Stop accepting new events and wait until any in-flight
    /// schedule has left the loop, so the caller can close the loop
    /// without racing a worker thread.  Must not be called under the
    /// GIL; use it from async teardown.
    pub(crate) fn quiesce(&self) {
        self.close();
        let _guard = self.schedule_lock.lock().unwrap();
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

    /// Queue a session event for delivery and schedule the loop
    /// drain if this caller flipped the scheduled flag.  The GIL is
    /// attached only for the single `call_soon_threadsafe` signal;
    /// the event is materialized into a Python object at drain time.
    fn schedule(&self, event: SessionEvent) {
        if self.event_callback.is_none() || self.closed.load(Ordering::SeqCst) {
            return;
        }
        let _guard = self.schedule_lock.lock().unwrap();
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let result: PyResult<()> = Python::attach(|py| {
            if self.queue.push(QueuedEvent::Session(event)) {
                let loop_guard = self.loop_.lock().unwrap();
                let Some(loop_) = loop_guard.as_ref() else {
                    self.queue.unschedule();
                    return Ok(());
                };
                let loop_py = loop_.clone_ref(py);
                drop(loop_guard);
                // Never marshal onto a closed or tearing-down loop:
                // writing to its self-pipe from a worker thread
                // crashes on Windows.
                if !crate::python::pyfuture::loop_accepts_callbacks(
                    loop_py.bind(py),
                )? {
                    self.queue.unschedule();
                    return Ok(());
                }
                let callback = self
                    .event_callback
                    .as_ref()
                    .expect("checked above")
                    .bind(py);
                let drainer = Py::new(
                    py,
                    EventDrainer {
                        queue: self.queue.clone(),
                        events: Some(callback.clone().unbind()),
                        progress: self.progress_callback.clone(),
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
            log::warn!("failed to schedule session event: {err}");
        }
    }

    /// Queue a progress report for delivery.
    fn schedule_progress(&self, op_index: i64) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let _guard = self.schedule_lock.lock().unwrap();
        if self.closed.load(Ordering::SeqCst)
            || self.progress_callback.lock().unwrap().is_none()
        {
            return;
        }
        let result: PyResult<()> = Python::attach(|py| {
            let progress_guard = self.progress_callback.lock().unwrap();
            if progress_guard.is_none() {
                return Ok(());
            }
            drop(progress_guard);
            let loop_guard = self.loop_.lock().unwrap();
            let Some(loop_) = loop_guard.as_ref() else {
                return Ok(());
            };
            let loop_py = loop_.clone_ref(py);
            drop(loop_guard);
            // Never marshal onto a closed or tearing-down loop (see
            // schedule()).
            if !crate::python::pyfuture::loop_accepts_callbacks(
                loop_py.bind(py),
            )? {
                return Ok(());
            }
            if self.queue.push(QueuedEvent::Progress(op_index)) {
                let drainer = Py::new(
                    py,
                    EventDrainer {
                        queue: self.queue.clone(),
                        events: self
                            .event_callback
                            .as_ref()
                            .map(|cb| cb.clone_ref(py)),
                        progress: self.progress_callback.clone(),
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
}

impl SessionEvents for PyEvents {
    fn state_changed(&self, state: &CoreDeviceState) {
        self.schedule(SessionEvent::StateChanged {
            state: PyDeviceState::new(state.clone()),
        });
    }

    fn connection_status(
        &self,
        status: TransportStatus,
        message: Option<&str>,
    ) {
        self.schedule(SessionEvent::ConnectionStatusChanged {
            status: status.name().to_string(),
            message: message.map(str::to_string),
        });
    }

    fn command_status(&self, status: TransportStatus, message: Option<&str>) {
        self.schedule(SessionEvent::CommandStatusChanged {
            status: status.name().to_string(),
            message: message.map(str::to_string),
        });
    }

    fn job_finished(&self) {
        self.schedule(SessionEvent::JobFinished {});
    }

    fn probe_status(&self, message: &str) {
        self.schedule(SessionEvent::ProbeStatusChanged {
            message: message.to_string(),
        });
    }

    fn wcs_updated(&self, offsets: &HashMap<String, (f64, f64, f64)>) {
        self.schedule(SessionEvent::WcsUpdated {
            offsets: offsets.clone(),
        });
    }

    fn config_changed(&self, key: &str, value: i64) {
        self.schedule(SessionEvent::ConfigChanged {
            key: key.to_string(),
            value,
        });
    }

    fn command_done(&self, op_index: i64) {
        self.schedule_progress(op_index);
    }

    fn raw_rx(&self, line: &str) {
        self.schedule(SessionEvent::RawRx {
            line: line.to_string(),
        });
    }

    fn raw_tx(&self, line: &str) {
        self.schedule(SessionEvent::RawTx {
            line: line.to_string(),
        });
    }
}

impl EventDrainer {
    /// Deliver each queued event to its own callback: session
    /// events to the event callback, progress reports to the
    /// progress callback.
    fn take_and_deliver(&self, py: Python<'_>) {
        for event in self.queue.take() {
            match event {
                QueuedEvent::Session(event) => {
                    let Some(callback) = self.events.as_ref() else {
                        continue;
                    };
                    let obj = match event.into_pyobject(py) {
                        Ok(obj) => obj.into_any().unbind(),
                        Err(err) => {
                            log::warn!(
                                "failed to materialize session event: {err}"
                            );
                            continue;
                        }
                    };
                    if let Err(err) = callback.bind(py).call1((obj,)) {
                        log::warn!("event callback failed: {err}");
                    }
                }
                QueuedEvent::Progress(op_index) => {
                    let progress_guard = self.progress.lock().unwrap();
                    let Some(callback) = progress_guard.as_ref() else {
                        continue;
                    };
                    let callback = callback.clone_ref(py);
                    drop(progress_guard);
                    if let Err(err) = callback.bind(py).call1((op_index,)) {
                        log::warn!("progress callback failed: {err}");
                    }
                }
            }
        }
    }
}
