//! The shared tokio runtime backing all sessions.
//!
//! A single process-wide runtime keeps task spawning cheap and
//! avoids interpreter-shutdown deadlocks: per-session runtimes would
//! have to be joined during `Drop`, which cannot block on tasks that
//! need the GIL while Python is finalizing.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    spawn_gil_watchdog(&runtime);
    runtime
});

/// Heartbeat that periodically attaches to the GIL and prints a
/// line.  If the interpreter ever deadlocks on the GIL, the
/// heartbeats stop, which the CI logs will show.
fn spawn_gil_watchdog(runtime: &tokio::runtime::Runtime) {
    runtime.spawn(async {
        let start = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let secs = start.elapsed().as_secs();
            let attached_at = std::time::Instant::now();
            pyo3::Python::attach(|py| {
                let _ = py;
                eprintln!(
                    "[gil-watchdog] alive after {secs}s (attach took \
                     {:?})",
                    attached_at.elapsed()
                );
            });
        }
    });
}

pub(crate) fn handle() -> &'static tokio::runtime::Runtime {
    &RUNTIME
}
