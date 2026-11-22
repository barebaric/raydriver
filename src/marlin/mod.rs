//! Marlin protocol implementation.

use crate::core::remote::registry::{
    CensusKind, ProbeSpec, ProtocolDescriptor,
};

pub mod parser;
pub mod probe;
pub mod session;

pub const DESCRIPTOR: ProtocolDescriptor = ProtocolDescriptor {
    name: "marlin",
    label: "Marlin",
    dialect_family: "marlin",
    connection: "serial",
    config_keys: &["port", "baudrate"],
    extra_methods: &["clear_alarm", "detect_unit_system", "probe_device"],
    buffer_stats: false,
    census: CensusKind::Marlin,
    probe: Some(ProbeSpec {
        command: "M115",
        signature: "FIRMWARE",
    }),
};
