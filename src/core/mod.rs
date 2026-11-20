//! Shared device-core primitives.
//!
//! Everything here is protocol-neutral: byte transports, the device
//! value types, dialect command-template formatting, the session
//! event/error primitives, and the transport-level discovery
//! scanners.  The protocol modules (`grbl`, `smoothie`, `marlin`)
//! build their sessions on top of this layer.

pub mod dialect;
pub mod discovery;
pub mod http;
pub mod remote;
pub mod session;
pub mod transport;
pub mod types;
pub mod ws;
