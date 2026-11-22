//! The crate's protocol registry.
//!
//! Protocol modules export a [`ProtocolDescriptor`]; this module is
//! the single composition point that assembles them into the
//! [`Registry`] consumed by the daemon and the Python bindings.
//! `core` stays free of protocol names.

use std::sync::LazyLock;

use crate::core::remote::registry::Registry;

static REGISTRY: LazyLock<Registry> = LazyLock::new(|| {
    Registry::new(&[
        crate::grbl::DESCRIPTOR,
        crate::grbl::SIMPLE_DESCRIPTOR,
        crate::esp3d::DESCRIPTOR,
        crate::smoothie::DESCRIPTOR,
        crate::octoprint::DESCRIPTOR,
        crate::marlin::DESCRIPTOR,
    ])
});

/// The registry of protocols a daemon can host.
pub fn registry() -> &'static Registry {
    &REGISTRY
}
