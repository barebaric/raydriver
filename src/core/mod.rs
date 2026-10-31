//! Shared device-core primitives.
//!
//! Everything here is protocol-neutral: byte transports, the device
//! value types, dialect command-template formatting, and the session
//! event/error primitives.  The protocol modules (`grbl`,
//! `smoothie`, `marlin`) build their sessions on top of this layer.

pub mod dialect;
pub mod session;
pub mod transport;
pub mod types;
