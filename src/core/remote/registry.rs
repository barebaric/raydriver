//! Protocol registry: what each hosted session protocol speaks.
//!
//! Protocol modules export a [`ProtocolDescriptor`]; the crate-root
//! [`crate::protocols`] module assembles them into a [`Registry`].
//! The daemon validates calls against the hosted protocol's surface
//! and `protocol/list` serves this metadata to clients, so clients
//! never hardcode protocol knowledge.

use std::collections::BTreeSet;

use serde_json::{json, Value};

/// How a protocol's device census is represented to Python.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CensusKind {
    Grbl,
    Marlin,
    Smoothie,
    OctoPrint,
}

/// A command and response signature for protocol auto-detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeSpec {
    pub command: &'static str,
    pub signature: &'static str,
}

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
    pub census: CensusKind,
    pub probe: Option<ProbeSpec>,
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

/// The protocol-neutral part of a protocol's registry entry.
///
/// Each protocol module exports one descriptor. Method and property
/// sets are derived from the shared surface plus the descriptor's
/// `extra_methods` and `buffer_stats` flag.
#[derive(Debug, Clone, Copy)]
pub struct ProtocolDescriptor {
    pub name: &'static str,
    pub label: &'static str,
    pub dialect_family: &'static str,
    pub connection: &'static str,
    pub config_keys: &'static [&'static str],
    pub extra_methods: &'static [&'static str],
    pub buffer_stats: bool,
    pub census: CensusKind,
    pub probe: Option<ProbeSpec>,
}

impl ProtocolDescriptor {
    pub fn info(self) -> ProtocolInfo {
        let mut methods: BTreeSet<&'static str> =
            COMMON_METHODS.iter().copied().collect();
        methods.extend(self.extra_methods.iter().copied());
        let properties = if self.buffer_stats {
            BUFFER_PROPERTIES.iter().copied().collect()
        } else {
            COMMON_PROPERTIES.iter().copied().collect()
        };
        ProtocolInfo {
            name: self.name,
            label: self.label,
            dialect_family: self.dialect_family,
            connection: self.connection,
            config_keys: self.config_keys,
            methods,
            properties,
            census: self.census,
            probe: self.probe,
        }
    }
}

pub const COMMON_METHODS: &[&str] = &[
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
];

const COMMON_PROPERTIES: &[&str] = &["state", "job_running", "resource_uri"];
const BUFFER_PROPERTIES: &[&str] = &[
    "state",
    "job_running",
    "resource_uri",
    "buffer_count",
    "rx_buffer_size",
    "pending_commands",
];

/// The protocols a daemon can host.
#[derive(Debug, Default, Clone)]
pub struct Registry {
    protocols: Vec<ProtocolInfo>,
}

impl Registry {
    pub fn new(descriptors: &[ProtocolDescriptor]) -> Self {
        Self {
            protocols: descriptors.iter().map(|d| d.info()).collect(),
        }
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.protocols.iter().map(|info| info.name).collect()
    }

    pub fn is_known(&self, name: &str) -> bool {
        self.protocols.iter().any(|info| info.name == name)
    }

    pub fn info(&self, name: &str) -> Option<&ProtocolInfo> {
        self.protocols.iter().find(|info| info.name == name)
    }

    /// Commands to try when resolving an unconfigured device.
    pub fn probe_candidates(&self) -> Vec<(&'static str, ProbeSpec)> {
        self.protocols
            .iter()
            .filter_map(|info| info.probe.map(|probe| (info.name, probe)))
            .collect()
    }

    /// The `protocol/list` payload: every protocol's metadata.
    pub fn payload(&self) -> Value {
        json!({
            "protocols": self.protocols
                .iter()
                .map(ProtocolInfo::to_json)
                .collect::<Vec<_>>(),
        })
    }
}
