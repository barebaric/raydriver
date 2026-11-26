//! Connection supervision state machine for Ruida sessions.
//!
//! This mirrors ``ruida_pa.ruidadriver.rd_status``: one connection is
//! first opened, then pinged, then polled. A failed connection moves to
//! the next configured link candidate; once candidates are exhausted
//! the session sleeps before starting again.
//!
//! ```text
//! Connecting -> WaitToPing -> SendPing -> PingReply -> WaitToPoll
//! WaitToPoll -> SendQuery -> ReplyPending -> WaitToPoll
//! ReplyPending -> SendQuery        (Busy: ACKed without a reply)
//! ReplyPending -> Resync           (query timeout budget exhausted)
//! Resync -> Connecting             (recover a stale TCP stream)
//! any link error -> Connecting
//! ```

use crate::ruida::link::TransportKind;

/// One state of the connection supervisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionState {
    Connecting,
    WaitToPing,
    SendPing,
    PingReply,
    WaitToPoll,
    SendQuery,
    ReplyPending,
    Resync,
}

/// The ordered link candidates for one session.
///
/// USB is preferred when configured; the network candidate is the
/// fallback. This ordering is what makes an unplugged USB cable move
/// the session to Ethernet on the next reconnect attempt.
pub(crate) struct ConnectionMonitor {
    candidates: Vec<TransportKind>,
    index: usize,
    state: ConnectionState,
}

impl ConnectionMonitor {
    pub(crate) fn new(candidates: Vec<TransportKind>) -> Self {
        Self {
            candidates,
            index: 0,
            state: ConnectionState::Connecting,
        }
    }

    pub(crate) fn set_state(&mut self, state: ConnectionState) {
        self.state = state;
    }

    pub(crate) fn current_candidate(&self) -> Option<TransportKind> {
        self.candidates.get(self.index).copied()
    }

    /// Moves to the next candidate. Returns false when exhausted.
    pub(crate) fn advance_candidate(&mut self) -> bool {
        if self.index + 1 >= self.candidates.len() {
            return false;
        }
        self.index += 1;
        self.state = ConnectionState::Connecting;
        true
    }
}
