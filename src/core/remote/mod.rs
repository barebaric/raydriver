//! Remote sessions: a device-host daemon and a client proxy.

pub mod client;
pub mod daemon;
pub mod inspect;
pub mod mdns;
pub mod protocol;
pub mod registry;
pub mod session;
pub mod transport;

/// The raydriver version reported in the daemon welcome.
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
