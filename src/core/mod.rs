//! Shared device-core primitives.
//!
//! Everything here is protocol-neutral: byte transports, the device
//! value types, dialect command-template formatting, the session
//! event/error primitives, and the transport-level discovery
//! scanners.  The protocol modules (`grbl`, `smoothie`, `marlin`,
//! and the network-native `esp3d`/`octoprint`/`xtool`)
//! build their sessions on top of this layer.
//!
//! Host-dependent pieces are feature-gated so a wasm build can omit
//! them entirely: `http`/`ws` and `remote` behind `net` (plus the
//! `mdns`/`remote-daemon` subpieces), leaving the pure value types,
//! dialects, session primitives and transport registry.

pub mod dialect;
pub mod discovery;
#[cfg(feature = "net")]
pub mod http;
pub(crate) mod line_session;
#[cfg(any(feature = "net", feature = "mdns"))]
pub mod remote;
pub mod session;
pub mod transport;
pub mod types;
#[cfg(feature = "net")]
pub mod ws;
