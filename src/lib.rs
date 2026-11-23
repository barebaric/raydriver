//! Rust-native machine drivers.
//!
//! The crate is split into a protocol-neutral core and protocol
//! modules built on top of it:
//!
//! - [`core`]: shared primitives — byte transports, device value
//!   types, dialect command-template formatting, the session
//!   event/error primitives, and the transport-level discovery
//!   scanners (serial capture, mDNS browse, subnet TCP scan).
//! - [`epilog`]: the Epilog laser print-job session (LPD).
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
//!
//! Build features slice out what a wasm host physically cannot do
//! (`mdns`, the remote daemon listener, the remote client, OS serial
//! ports, the tungstenite-based M2 session); everything else —
//! including the HTTP/WS device sessions — compiles and runs on
//! wasm against embedder-provided streams (see docs/wasm.md).  The
//! M2 session's TLS roots are chosen by `tls-native-roots` (default)
//! or `tls-webpki-roots`.

// The M2 WebSocket client needs a rustls trust store, selected at
// build time.  Both provider features imply `m2`; this guard catches
// `--no-default-features --features m2` with an actionable message
// instead of a cryptic missing-`Connector` variant error.
#[cfg(all(
    feature = "m2",
    not(any(feature = "tls-native-roots", feature = "tls-webpki-roots"))
))]
compile_error!(
    "the `m2` feature requires a TLS root provider: enable \
     `tls-native-roots` (OS trust store, the default) or \
     `tls-webpki-roots` (bundled Mozilla roots)"
);

pub mod core;
pub mod epilog;
pub mod esp3d;
pub mod grbl;
pub mod marlin;
pub mod octoprint;
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
