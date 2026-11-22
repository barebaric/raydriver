//! Protocol registry: what each hosted session protocol speaks.
//!
//! [`ProtocolInfo`] describes one protocol's surface; the daemon
//! validates calls against it and `protocol/list` serves the
//! metadata to clients so they never hardcode protocol knowledge.
//!
//! Core stays protocol-neutral: the *set* of hostable protocols —
//! and their auto-detection fingerprints — is supplied by the
//! embedding application as a [`Registry`] at daemon construction.
//! In this crate the Python bindings build the concrete table (see
//! `python::protocols::protocol_registry`); a pure-Rust embedder
//! builds its own from the same primitives.

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
    /// Command + response signature used to auto-detect a device
    /// speaking this protocol (daemon device probing).  `None` = the
    /// protocol is not auto-detectable.
    pub probe: Option<ProbeFingerprint>,
}

/// A protocol's auto-detection fingerprint.
#[derive(Debug, Clone)]
pub struct ProbeFingerprint {
    /// The interactive command sent to the device.
    pub command: &'static str,
    /// A marker expected in the response.
    pub signature: &'static str,
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

/// The method surface every hosted session implements, plus the
/// protocol's extras.
pub fn methods(extra: &[&'static str]) -> BTreeSet<&'static str> {
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

/// The property surface every hosted session implements.
pub fn common_properties() -> BTreeSet<&'static str> {
    ["state", "job_running", "resource_uri"]
        .into_iter()
        .collect()
}

/// The property surface for protocols with RX-buffer accounting.
pub fn buffer_properties() -> BTreeSet<&'static str> {
    let mut set = common_properties();
    set.extend(["buffer_count", "rx_buffer_size", "pending_commands"]);
    set
}

/// The set of protocols a daemon can host, supplied by the embedding
/// application at daemon construction.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    protocols: Vec<ProtocolInfo>,
}

impl Registry {
    /// Build a registry from the protocol table, in serving order.
    pub fn new(protocols: Vec<ProtocolInfo>) -> Self {
        Self { protocols }
    }

    /// True when *name* is hostable.
    pub fn is_known(&self, name: &str) -> bool {
        self.protocols.iter().any(|info| info.name == name)
    }

    /// Look up a protocol's metadata.
    pub fn protocol_info(&self, name: &str) -> Option<ProtocolInfo> {
        self.protocols
            .iter()
            .find(|info| info.name == name)
            .cloned()
    }

    /// The protocol table, in serving order.
    pub fn protocols(&self) -> &[ProtocolInfo] {
        &self.protocols
    }

    /// The `protocol/list` payload: every protocol's metadata.
    pub fn protocols_payload(&self) -> Value {
        json!({
            "protocols": self
                .protocols
                .iter()
                .map(|info| info.to_json())
                .collect::<Vec<_>>(),
        })
    }
}
