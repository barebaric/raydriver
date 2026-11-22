//! The OctoPrint protocol module: a print-server API session.

use crate::core::remote::registry::{CensusKind, ProtocolDescriptor};

pub mod probe;
pub mod session;

pub const DESCRIPTOR: ProtocolDescriptor = ProtocolDescriptor {
    name: "octoprint",
    label: "OctoPrint",
    dialect_family: "grbl",
    connection: "network",
    config_keys: &["host", "port", "path", "api_key"],
    extra_methods: &[
        "clear_alarm",
        "probe_device",
        "probe_info",
        "read_wcs_offsets",
        "read_parser_state",
        "run_probe_cycle",
        "set_hold",
    ],
    buffer_stats: false,
    census: CensusKind::OctoPrint,
    probe: None,
};
