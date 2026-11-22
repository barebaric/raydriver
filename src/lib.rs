//! Rust-native machine drivers.
//!
//! The crate is split into a protocol-neutral core and protocol
//! modules built on top of it:
//!
//! - [`core`]: shared primitives — byte transports, device value
//!   types, dialect command-template formatting, the session
//!   event/error primitives, and the transport-level discovery
//!   scanners (serial capture, mDNS browse, subnet TCP scan).
//! - [`esp3d`]: the ESP3D / FluidNC transport for GRBL over HTTP and
//!   WebSocket.
//! - [`grbl`]: the GRBL protocol stack (parsing, flow control, the
//!   full session and the minimal ping-pong session).
//! - [`smoothie`]: the Smoothieware session.
//! - [`xtool`]: the xTool proprietary WiFi-API session.
//! - [`marlin`]: the Marlin session.
//!
//! The crate is layered: `core` and the protocol modules are pure
//! Rust and must not depend on PyO3; the optional `python` feature
//! provides bindings that mirror this structure.

pub mod core;
pub mod esp3d;
pub mod grbl;
pub mod marlin;
pub mod octoprint;
pub mod protocols;
pub mod smoothie;
pub mod xtool;

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
