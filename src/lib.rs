//! Rust-native machine drivers.
//!
//! Currently contains the GRBL protocol stack:
//!
//! - [`grbl::types`]: shared value types (device state, statuses,
//!   errors).
//! - [`grbl::errors`]: Grbl error and alarm code tables.
//! - [`grbl::parser`]: response parsers (status reports, settings,
//!   probe results, …).
//! - [`grbl::dialect`]: command templates and `str.format`-style
//!   formatting.
//! - [`grbl::flow`]: character-counting RX-buffer flow control.
//! - [`grbl::transport`]: async byte transports (serial, telnet,
//!   mock).
//! - [`grbl::session`]: the connection/streaming state machines.
//!
//! The crate is layered: `grbl` is pure Rust and must not depend on
//! PyO3; the optional `python` feature provides bindings that
//! mirror this structure.

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
