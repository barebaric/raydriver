//! Smoothie protocol implementation.

use crate::core::remote::registry::{CensusKind, ProtocolDescriptor};

pub mod probe;
pub mod session;

pub const DESCRIPTOR: ProtocolDescriptor = ProtocolDescriptor {
    name: "smoothie",
    label: "Smoothieware",
    dialect_family: "smoothieware",
    connection: "telnet",
    config_keys: &["host", "tcp_port"],
    extra_methods: &["set_hold", "clear_alarm", "probe_device"],
    buffer_stats: false,
    census: CensusKind::Smoothie,
    probe: None,
};
