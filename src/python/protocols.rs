//! The `raydriver (root); the Python file at protocols.py is the shim` submodule: the single source of truth
//! for what each session protocol speaks and what it can do.
//!
//! The daemon hosts sessions by protocol; a client (or an
//! application such as Rayforge) learns a protocol's label,
//! capabilities and config shape from here — as info — instead of
//! hardcoding protocol knowledge. The registry is data beside the
//! session bindings: every session implements the uniform method
//! surface, so a capability that is present but not *meaningful*
//! for a protocol (a settings reader that returns nothing) is
//! switched off explicitly via overrides.

use std::collections::HashMap;

use pyo3::exceptions::PyKeyError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use pyo3_stub_gen::derive::{
    gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods,
};

/// Methods every session implements, regardless of protocol.
const COMMON_METHODS: [&str; 15] = [
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

/// Properties every session exposes.
const COMMON_PROPERTIES: [&str; 3] = ["state", "job_running", "resource_uri"];

/// The methods the GRBL-family sessions add beyond the common
/// surface.
const GRBL_EXTRA: [&str; 8] = [
    "set_hold",
    "read_settings",
    "write_setting",
    "read_wcs_offsets",
    "read_parser_state",
    "run_probe_cycle",
    "detect_unit_system",
    "probe_device",
];

/// One protocol's registry entry.
struct ProtocolEntry {
    name: &'static str,
    label: &'static str,
    dialect_family: &'static str,
    connection: &'static str,
    config_keys: &'static [&'static str],
    extra_methods: &'static [&'static str],
    extra_properties: &'static [&'static str],
    /// Capabilities that are present on the uniform surface but not
    /// meaningful for this protocol.
    capability_overrides: &'static [(&'static str, bool)],
}

impl ProtocolEntry {
    fn methods(&self) -> Vec<String> {
        let mut methods: Vec<String> = COMMON_METHODS
            .iter()
            .chain(self.extra_methods.iter())
            .map(|method| (*method).to_string())
            .collect();
        methods.sort();
        methods
    }

    fn properties(&self) -> Vec<String> {
        let mut properties: Vec<String> = COMMON_PROPERTIES
            .iter()
            .chain(self.extra_properties.iter())
            .map(|property| (*property).to_string())
            .collect();
        properties.sort();
        properties
    }

    fn capabilities(
        &self,
        methods: &[String],
        properties: &[String],
    ) -> HashMap<&'static str, bool> {
        let has = |names: &[String], method: &str| {
            names.contains(&method.to_string())
        };
        let mut capabilities = HashMap::new();
        capabilities.insert("uses_gcode", true);
        capabilities.insert("supports_settings", has(methods, "read_settings"));
        capabilities
            .insert("supports_probing", has(methods, "run_probe_cycle"));
        capabilities.insert("supports_hold", has(methods, "set_hold"));
        capabilities
            .insert("supports_clear_alarm", has(methods, "clear_alarm"));
        capabilities.insert(
            "supports_unit_detection",
            has(methods, "detect_unit_system"),
        );
        capabilities
            .insert("reports_buffer_stats", has(properties, "buffer_count"));
        for (name, value) in self.capability_overrides {
            capabilities.insert(name, *value);
        }
        capabilities
    }
}

static PROTOCOLS: [ProtocolEntry; 6] = [
    ProtocolEntry {
        name: "grbl",
        label: "GRBL",
        dialect_family: "grbl",
        connection: "serial_or_telnet",
        config_keys: &["port", "baudrate", "host", "tcp_port"],
        extra_methods: &GRBL_EXTRA,
        extra_properties: &[
            "buffer_count",
            "rx_buffer_size",
            "pending_commands",
        ],
        capability_overrides: &[],
    },
    ProtocolEntry {
        name: "grbl_simple",
        label: "GRBL (simple)",
        dialect_family: "grbl",
        connection: "serial_or_telnet",
        config_keys: &["port", "baudrate", "host", "tcp_port"],
        extra_methods: &GRBL_EXTRA,
        extra_properties: &[],
        capability_overrides: &[],
    },
    ProtocolEntry {
        name: "esp3d",
        label: "FluidNC / ESP3D",
        dialect_family: "grbl",
        connection: "network",
        config_keys: &["host", "http_port", "ws_port", "protocol"],
        extra_methods: &GRBL_EXTRA,
        extra_properties: &[],
        capability_overrides: &[],
    },
    ProtocolEntry {
        name: "smoothie",
        label: "Smoothieware",
        dialect_family: "smoothieware",
        connection: "telnet",
        config_keys: &["host", "tcp_port"],
        extra_methods: &["set_hold", "clear_alarm", "probe_device"],
        extra_properties: &[],
        capability_overrides: &[],
    },
    ProtocolEntry {
        name: "octoprint",
        label: "OctoPrint",
        dialect_family: "grbl",
        connection: "network",
        config_keys: &["host", "port", "path", "api_key"],
        extra_methods: &[
            "clear_alarm",
            "probe_info",
            "read_wcs_offsets",
            "read_parser_state",
            "run_probe_cycle",
            "read_settings",
            "write_setting",
            "detect_unit_system",
            "set_hold",
        ],
        extra_properties: &[],
        // Every session implements the uniform surface; these
        // capabilities are present but not meaningful here.
        capability_overrides: &[
            ("supports_settings", false),
            ("supports_unit_detection", false),
        ],
    },
    ProtocolEntry {
        name: "marlin",
        label: "Marlin",
        dialect_family: "marlin",
        connection: "serial",
        config_keys: &["port", "baudrate"],
        extra_methods: &["clear_alarm", "detect_unit_system", "probe_device"],
        extra_properties: &[],
        capability_overrides: &[],
    },
];

fn find_entry(name: &str) -> PyResult<&'static ProtocolEntry> {
    PROTOCOLS
        .iter()
        .find(|entry| entry.name == name)
        .ok_or_else(|| PyKeyError::new_err(name.to_string()))
}

/// One protocol's metadata: label, config shape, method surface and
/// derived capabilities.
#[gen_stub_pyclass(module = "raydriver")]
#[pyclass(module = "raydriver", frozen)]
pub struct ProtocolInfo {
    entry: &'static ProtocolEntry,
}

#[gen_stub_pymethods]
#[pymethods]
impl ProtocolInfo {
    #[getter]
    fn name(&self) -> &str {
        self.entry.name
    }

    #[getter]
    fn label(&self) -> &str {
        self.entry.label
    }

    /// A hint clients can map to their own command-dialect
    /// definitions; it is not a driver selection.
    #[getter]
    fn dialect_family(&self) -> &str {
        self.entry.dialect_family
    }

    #[getter]
    fn connection(&self) -> &str {
        self.entry.connection
    }

    #[getter]
    fn config_keys(&self) -> Vec<&str> {
        self.entry.config_keys.to_vec()
    }

    /// The sorted method surface of the session.
    #[getter]
    fn methods(&self) -> Vec<String> {
        self.entry.methods()
    }

    /// The sorted property surface of the session.
    #[getter]
    fn properties(&self) -> Vec<String> {
        self.entry.properties()
    }

    /// What applications can do with the protocol, derived from the
    /// method/property surface so the two never drift apart.
    #[getter]
    fn capabilities(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let methods = self.entry.methods();
        let properties = self.entry.properties();
        let capabilities = self.entry.capabilities(&methods, &properties);
        let dict = PyDict::new(py);
        for (name, value) in capabilities {
            dict.set_item(name, value)?;
        }
        Ok(dict.unbind().into_any())
    }

    /// The wire form served by `protocol/list`.
    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        dict.set_item("name", self.entry.name)?;
        dict.set_item("label", self.entry.label)?;
        dict.set_item("dialect_family", self.entry.dialect_family)?;
        dict.set_item("connection", self.entry.connection)?;
        dict.set_item("config_keys", self.entry.config_keys)?;
        dict.set_item("capabilities", self.capabilities(py)?)?;
        dict.set_item("methods", self.entry.methods())?;
        dict.set_item("properties", self.entry.properties())?;
        Ok(dict.unbind().into_any())
    }
}

/// The registry as a name → metadata mapping.
#[gen_stub_pyfunction(module = "raydriver")]
#[pyfunction]
fn protocols(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    for entry in PROTOCOLS.iter() {
        let info = ProtocolInfo { entry };
        dict.set_item(entry.name, info)?;
    }
    Ok(dict.unbind().into_any())
}

/// Look up a protocol's metadata, raising `KeyError` if unknown.
#[gen_stub_pyfunction(module = "raydriver")]
#[pyfunction]
fn protocol_info(name: &str) -> PyResult<ProtocolInfo> {
    Ok(ProtocolInfo {
        entry: find_entry(name)?,
    })
}

/// The `protocol/list` payload: every protocol's metadata.
#[gen_stub_pyfunction(module = "raydriver")]
#[pyfunction]
fn protocols_payload(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let list = PyList::new(
        py,
        PROTOCOLS
            .iter()
            .map(|entry| {
                let info = ProtocolInfo { entry };
                info.to_dict(py)
            })
            .collect::<PyResult<Vec<_>>>()?,
    )?;
    let dict = PyDict::new(py);
    dict.set_item("protocols", list)?;
    Ok(dict.unbind().into_any())
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Registered on the root module: the Python file at
    // python/raydriver/protocols.py is the importable module and
    // delegates here, so a Rust submodule must not shadow it.
    m.add_function(wrap_pyfunction!(protocol_info, m)?)?;
    m.add_function(wrap_pyfunction!(protocols, m)?)?;
    m.add_function(wrap_pyfunction!(protocols_payload, m)?)?;
    Ok(())
}
