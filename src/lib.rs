//! Rust-native machine drivers.
//!
//! The crate is split into a protocol-neutral core and protocol
//! modules built on top of it:
//!
//! - [`core`]: shared primitives — byte transports, device value
//!   types, dialect command-template formatting, and the session
//!   event/error primitives.
//! - [`grbl`]: the GRBL protocol stack (parsing, flow control, the
//!   full session and the minimal ping-pong session).
//!
//! The crate is layered: `core` and the protocol modules are pure
//! Rust and must not depend on PyO3; the optional `python` feature
//! provides bindings that mirror this structure.

pub mod core;
pub mod grbl;

#[cfg(feature = "python")]
pub(crate) mod python;

#[cfg(feature = "python")]
use pyo3::prelude::*;

#[cfg(feature = "python")]
pyo3_stub_gen::define_stub_info_gatherer!(stub_info);

#[cfg(feature = "python")]
#[pymodule(gil_used = false)]
fn raydriver(m: &Bound<'_, PyModule>) -> PyResult<()> {
    python::register(m)?;
    Ok(())
}
