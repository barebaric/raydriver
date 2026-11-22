//! ESP3D / FluidNC devices: the GRBL protocol over an HTTP command
//! endpoint and a WebSocket response channel.

use crate::core::remote::registry::{CensusKind, ProtocolDescriptor};

pub mod transport;

pub const DESCRIPTOR: ProtocolDescriptor = ProtocolDescriptor {
    name: "esp3d",
    label: "FluidNC / ESP3D",
    dialect_family: "grbl",
    connection: "network",
    config_keys: &["host", "http_port", "ws_port", "protocol"],
    extra_methods: crate::grbl::EXTRA_METHODS,
    buffer_stats: false,
    census: CensusKind::Grbl,
    probe: None,
};
