//! Bridge for completing asyncio futures from the tokio runtime.
//!
//! Python async methods hand back an `asyncio.Future` that a Rust
//! future completes via `loop.call_soon_threadsafe` — mirroring the
//! pattern Rayforge's Python drivers use to wake waiters from
//! non-loop threads.  Nothing here ever blocks the event loop.

use std::future::Future;

use pyo3::prelude::*;

use super::runtime;

/// Spawn *fut* on the shared tokio runtime and return an asyncio
/// future that resolves with its result.
///
/// The future must be created while running on the event loop (i.e.
/// from an async Python call stack), which is the only supported
/// calling context.
pub(crate) fn spawn_future<F>(
    py: Python<'_>,
    fut: F,
) -> PyResult<Bound<'_, PyAny>>
where
    F: Future<Output = PyResult<Py<PyAny>>> + Send + 'static,
{
    let asyncio = py.import("asyncio")?;
    let loop_ = asyncio.call_method0("get_running_loop")?;
    let future = loop_.call_method1("create_future", ())?;
    let loop_py: Py<PyAny> = loop_.unbind();
    let future_py: Py<PyAny> = future.clone().unbind();

    runtime::handle().spawn(async move {
        let outcome = fut.await;
        let schedule: PyResult<()> = Python::attach(|py| {
            let loop_bound = loop_py.bind(py);
            // The awaiting loop may be gone or tearing down (e.g. the
            // test ended); scheduling onto it crashes on Windows.
            if !loop_accepts_callbacks(loop_bound)? {
                return Ok(());
            }
            // The awaiting task may have been cancelled while the
            // Rust side was still running (e.g. app shutdown);
            // completing a done future raises InvalidStateError in
            // the loop callback, so guard on done() at call time.
            let complete = py.eval(
                c"lambda f, v, e: None if f.done() else \
                  (f.set_exception(e) if e is not None else f.set_result(v))",
                None,
                None,
            )?;
            match outcome {
                Ok(value) => {
                    loop_bound.call_method1(
                        "call_soon_threadsafe",
                        (complete, future_py, value, py.None()),
                    )?;
                }
                Err(err) => {
                    let exc = err.value(py).to_owned();
                    loop_bound.call_method1(
                        "call_soon_threadsafe",
                        (complete, future_py, py.None(), exc),
                    )?;
                }
            }
            Ok(())
        });
        if let Err(err) = schedule {
            Python::attach(|py| err.print(py));
        }
    });
    Ok(future)
}

/// Whether *loop* can still receive threadsafe wake-ups: not closed
/// and running. ``close()`` only runs on a non-running loop and frees
/// the self-pipe while a worker's wake-up write may be in flight (the
/// socket send releases the GIL), which crashes on Windows — so only
/// schedule onto a live, running loop.
pub(crate) fn loop_accepts_callbacks(
    loop_bound: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    if loop_bound.call_method0("is_closed")?.is_truthy()? {
        return Ok(false);
    }
    loop_bound.call_method0("is_running")?.is_truthy()
}

/// A Python `None` as an owned `Py<PyAny>` (created under the GIL).
pub(crate) fn none_obj(py: Python<'_>) -> Py<PyAny> {
    ().into_pyobject(py)
        .expect("None conversion")
        .unbind()
        .into_any()
}
