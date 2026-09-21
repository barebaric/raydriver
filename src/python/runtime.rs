//! The shared tokio runtime backing all sessions.
//!
//! A single process-wide runtime keeps task spawning cheap and
//! avoids interpreter-shutdown deadlocks: per-session runtimes would
//! have to be joined during `Drop`, which cannot block on tasks that
//! need the GIL while Python is finalizing.

use std::sync::LazyLock;

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
});

pub(crate) fn handle() -> &'static tokio::runtime::Runtime {
    &RUNTIME
}
