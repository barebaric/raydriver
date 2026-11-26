//! The Ruida laser controller protocol.
//!
//! Ruida controllers (RDC6442S, RDC8445S and relatives) speak a
//! binary, 7-bit packet protocol over USB, UDP or TCP. Jobs are
//! written as rpascript command streams; live motion and job control
//! use the same commands outside a job.
//!
//! The layers mirror the Python ``ruida-pa`` implementation:
//!
//! - :mod:`protocol` — command/memory tables, handshake bytes and status bits.
//! - :mod:`codec` — 7-bit packing, swizzle, checksums and rpascript encoding.
//! - :mod:`link` — raw USB/UDP/TCP byte links.
//! - :mod:`transport` — packet framing and the ACK state machine.
//! - :mod:`monitor` — connection, ping, poll and link-candidate state machine.
//! - :mod:`status` — typed ``GET_SETTING`` reply decoding.
//! - :mod:`driver` — session facade and job/live-motion commands.
//!
//! The protocol layers here are pure Rust and do not depend on PyO3.

pub mod codec;
pub mod driver;
pub mod gluescript;
pub mod link;
mod monitor;
pub mod protocol;
pub mod status;
pub mod transport;
