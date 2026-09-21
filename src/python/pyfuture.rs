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
            match outcome {
                Ok(value) => {
                    let set_result = future_py.getattr(py, "set_result")?;
                    loop_bound.call_method1(
                        "call_soon_threadsafe",
                        (set_result, value),
                    )?;
                }
                Err(err) => {
                    let set_exception =
                        future_py.getattr(py, "set_exception")?;
                    let exc = err.value(py).to_owned();
                    loop_bound.call_method1(
                        "call_soon_threadsafe",
                        (set_exception, exc),
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

/// A Python `None` as an owned `Py<PyAny>` (created under the GIL).
pub(crate) fn none_obj(py: Python<'_>) -> Py<PyAny> {
    ().into_pyobject(py)
        .expect("None conversion")
        .unbind()
        .into_any()
}
