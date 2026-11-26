//! The Ruida laser controller protocol.
//!
//! Ruida controllers (RDC6442S, RDC8445S and relatives) speak a
//! binary, 7-bit packet protocol over USB, UDP or TCP. Jobs are
//! written as rpascript command streams; live motion and job control
//! use the same commands outside a job.
//!
//! The module tree mirrors the Python ``ruida-pa`` package:
//!
//! - :mod:`protocols` — protocol tables and parser definitions.
//! - :mod:`rpalib` — swizzling and transcoding primitives.
//! - :mod:`rpascript` — rpascript parsing and encoding.
//! - :mod:`ruidadriver` — transport, session/status and driver layers.
//!
//! The protocol layers here are pure Rust and do not depend on PyO3.

pub mod protocols;
pub mod rpalib;
pub mod rpascript;
pub mod ruidadriver;
