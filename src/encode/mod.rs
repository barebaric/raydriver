//! Machine-code encoders for specific devices.
//!
//! These implement [`raygeo::ops::convert::Encoder`] and render an
//! `Ops` sequence into a device's job format: G-code dialects and the
//! Epilog print-job language.
//!
//! The encoders live here rather than in `raygeo` because their output
//! is machine-protocol knowledge, not geometry. The crate is pure Rust
//! and compiles for every target, including wasm, against raygeo's
//! core. The Python bindings under `src/python/encode/` additionally
//! expose each spec through the ``__raygeo_encoder__()`` capsule
//! handoff, so raygeo's pipeline can run an encoder it does not define.

pub mod epilog;
pub mod gcode;
pub mod gcode_types;
