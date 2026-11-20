//! Protocol registry: what each hosted session protocol speaks.
//!
//! The daemon validates a call against the specific protocol's
//! surface, and `protocol/list` serves this metadata to clients so
//! they never hardcode protocol knowledge.

use std::collections::BTreeSet;

use serde_json::{json, Value};

/// Metadata describing one session protocol.
#[derive(Debug, Clone)]
pub struct ProtocolInfo {
    pub name: &'static str,
    pub label: &'static str,
    pub dialect_family: &'static str,
    pub connection: &'static str,
    pub config_keys: &'static [&'static str],
    pub methods: BTreeSet<&'static str>,
    pub properties: BTreeSet<&'static str>,
}

impl ProtocolInfo {
    pub fn capabilities(&self) -> Value {
        json!({
            "uses_gcode": true,
            "supports_settings": self.methods.contains("read_settings"),
            "supports_probing": self.methods.contains("run_probe_cycle"),
            "supports_hold": self.methods.contains("set_hold"),
            "supports_clear_alarm": self.methods.contains("clear_alarm"),
            "supports_unit_detection":
                self.methods.contains("detect_unit_system"),
            "reports_buffer_stats": self.properties.contains("buffer_count"),
        })
    }

    pub fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "label": self.label,
            "dialect_family": self.dialect_family,
            "connection": self.connection,
            "config_keys": self.config_keys,
            "capabilities": self.capabilities(),
            "methods": self.methods.iter().collect::<Vec<_>>(),
            "properties": self.properties.iter().collect::<Vec<_>>(),
        })
    }
}

fn methods(extra: &[&'static str]) -> BTreeSet<&'static str> {
    let mut set: BTreeSet<&'static str> = [
        "connect",
        "disconnect",
        "run",
        "run_raw",
        "cancel",
        "execute_command",
        "execute_interactive_command",
        "home",
        "move_to",
        "jog",
        "select_tool",
        "set_power",
        "set_focus_power",
        "set_wcs_offset",
        "update_dialect",
    ]
    .into_iter()
    .collect();
    set.extend(extra.iter().copied());
    set
}

fn common_properties() -> BTreeSet<&'static str> {
    ["state", "job_running", "resource_uri"]
        .into_iter()
        .collect()
}

fn buffer_properties() -> BTreeSet<&'static str> {
    let mut set = common_properties();
    set.extend(["buffer_count", "rx_buffer_size", "pending_commands"]);
    set
}

const GRBL_EXTRA: &[&str] = &[
    "set_hold",
    "read_settings",
    "write_setting",
    "read_wcs_offsets",
    "read_parser_state",
    "run_probe_cycle",
    "detect_unit_system",
];

fn build(name: &str) -> ProtocolInfo {
    match name {
        "grbl" => ProtocolInfo {
            name: "grbl",
            label: "GRBL",
            dialect_family: "grbl",
            connection: "serial_or_telnet",
            config_keys: &["port", "baudrate", "host", "tcp_port"],
            methods: methods(GRBL_EXTRA),
            properties: buffer_properties(),
        },
        "grbl_simple" => ProtocolInfo {
            name: "grbl_simple",
            label: "GRBL (simple)",
            dialect_family: "grbl",
            connection: "serial_or_telnet",
            config_keys: &["port", "baudrate", "host", "tcp_port"],
            methods: methods(GRBL_EXTRA),
            properties: common_properties(),
        },
        "esp3d" => ProtocolInfo {
            name: "esp3d",
            label: "FluidNC / ESP3D",
            dialect_family: "grbl",
            connection: "network",
            config_keys: &["host", "http_port", "ws_port", "protocol"],
            methods: methods(GRBL_EXTRA),
            properties: common_properties(),
        },
        "smoothie" => ProtocolInfo {
            name: "smoothie",
            label: "Smoothieware",
            dialect_family: "smoothieware",
            connection: "telnet",
            config_keys: &["host", "tcp_port"],
            methods: methods(&["set_hold", "clear_alarm"]),
            properties: common_properties(),
        },
        _ => ProtocolInfo {
            name: "marlin",
            label: "Marlin",
            dialect_family: "marlin",
            connection: "serial",
            config_keys: &["port", "baudrate"],
            methods: methods(&["clear_alarm", "detect_unit_system"]),
            properties: common_properties(),
        },
    }
}

/// The protocols the daemon can host.
pub fn names() -> [&'static str; 5] {
    ["grbl", "grbl_simple", "esp3d", "smoothie", "marlin"]
}

pub fn is_known(name: &str) -> bool {
    names().contains(&name)
}

/// Look up a protocol's metadata.
pub fn protocol_info(name: &str) -> Option<ProtocolInfo> {
    if is_known(name) {
        Some(build(name))
    } else {
        None
    }
}

/// The `protocol/list` payload: every protocol's metadata.
pub fn protocols_payload() -> Value {
    json!({
        "protocols": names()
            .into_iter()
            .filter_map(protocol_info)
            .map(|info| info.to_json())
            .collect::<Vec<_>>(),
    })
}
