//! Shared command/ack tracking for ping-pong sessions.
//!
//! A minimal session sends one line and waits for its `ok`/`error:`
//! acknowledgement. The command being tracked is the same object for
//! every protocol; only how many may be outstanding differs.

use std::sync::Mutex;

use super::session::AsyncEvent;

/// One sent command awaiting its acknowledgement.
pub struct PendingCommand {
    lines: Mutex<Vec<String>>,
    finished: AsyncEvent,
}

impl PendingCommand {
    pub fn new() -> Self {
        Self {
            lines: Mutex::new(Vec::new()),
            finished: AsyncEvent::new(),
        }
    }

    pub fn add_line(&self, line: &str) {
        self.lines.lock().unwrap().push(line.to_string());
    }

    pub fn finish(&self) {
        self.finished.set();
    }

    pub fn is_finished(&self) -> bool {
        self.finished.is_set()
    }

    pub fn lines(&self) -> Vec<String> {
        self.lines.lock().unwrap().clone()
    }

    pub async fn wait(&self) {
        self.finished.wait().await;
    }
}

impl Default for PendingCommand {
    fn default() -> Self {
        Self::new()
    }
}
