//! GRBL protocol implementation.

use crate::core::remote::registry::{
    CensusKind, ProbeSpec, ProtocolDescriptor,
};

pub mod errors;
pub mod flow;
pub mod parser;
pub mod probe;
pub mod session;
pub mod simple;

pub(crate) const EXTRA_METHODS: &[&str] = &[
    "set_hold",
    "read_settings",
    "write_setting",
    "read_wcs_offsets",
    "read_parser_state",
    "run_probe_cycle",
    "detect_unit_system",
    "probe_device",
];

const CONFIG_KEYS: &[&str] =
    &["port", "baudrate", "host", "tcp_port", "address", "channel"];

pub const DESCRIPTOR: ProtocolDescriptor = ProtocolDescriptor {
    name: "grbl",
    label: "GRBL",
    dialect_family: "grbl",
    connection: "serial_or_telnet_or_bluetooth",
    config_keys: CONFIG_KEYS,
    extra_methods: EXTRA_METHODS,
    buffer_stats: true,
    census: CensusKind::Grbl,
    probe: Some(ProbeSpec {
        command: "$I",
        signature: "VER:",
    }),
};

pub const SIMPLE_DESCRIPTOR: ProtocolDescriptor = ProtocolDescriptor {
    name: "grbl_simple",
    label: "GRBL (simple)",
    dialect_family: "grbl",
    connection: "serial_or_telnet_or_bluetooth",
    config_keys: CONFIG_KEYS,
    extra_methods: EXTRA_METHODS,
    buffer_stats: false,
    census: CensusKind::Grbl,
    probe: None,
};
