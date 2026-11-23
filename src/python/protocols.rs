//! Python bindings for the protocol registry (`raydriver.protocols`).
//!
//! Registered as the `raydriver.protocols` submodule (stub-only: no
//! Python file shadows it).

use pyo3::exceptions::PyKeyError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::{
    gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods,
};

use crate::core::remote::registry as core;
use crate::python::core::json::value_to_py;

/// Metadata describing one session protocol.
///
/// ``capabilities`` is derived from the method/property surface so
/// the two never drift apart. ``dialect_family`` is a hint clients
/// can map to their own command-dialect definitions; it is not a
/// driver selection.
#[gen_stub_pyclass(module = "raydriver.protocols")]
#[pyclass(frozen, module = "raydriver.protocols")]
pub struct ProtocolInfo {
    inner: core::ProtocolInfo,
}

#[gen_stub_pymethods]
#[pymethods]
impl ProtocolInfo {
    #[getter]
    fn name(&self) -> String {
        self.inner.name.to_string()
    }

    #[getter]
    fn label(&self) -> String {
        self.inner.label.to_string()
    }

    #[getter]
    fn dialect_family(&self) -> String {
        self.inner.dialect_family.to_string()
    }

    #[getter]
    fn connection(&self) -> String {
        self.inner.connection.to_string()
    }

    #[getter]
    fn config_keys(&self) -> Vec<String> {
        self.inner
            .config_keys
            .iter()
            .map(|k| k.to_string())
            .collect()
    }

    #[getter]
    fn methods(&self) -> Vec<String> {
        self.inner.methods.iter().map(|m| m.to_string()).collect()
    }

    #[getter]
    fn properties(&self) -> Vec<String> {
        self.inner
            .properties
            .iter()
            .map(|p| p.to_string())
            .collect()
    }

    #[getter]
    #[gen_stub(override_return_type(
        type_repr = "dict[builtins.str, builtins.bool]"
    ))]
    fn capabilities<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        Ok(value_to_py(py, &self.inner.capabilities())?.into_bound(py))
    }

    /// The wire form served by ``protocol/list``.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = value_to_py(py, &self.inner.to_json())?.into_bound(py);
        Ok(dict.cast_into::<PyDict>()?)
    }
}

/// The extra remote-callable methods the GRBL-family sessions
/// implement beyond the uniform surface.
const GRBL_EXTRA: &[&str] = &[
    "set_hold",
    "read_settings",
    "write_setting",
    "read_wcs_offsets",
    "read_parser_state",
    "run_probe_cycle",
    "detect_unit_system",
    "probe_device",
];

/// The concrete protocol table this crate serves.  Protocol
/// knowledge lives with the embedding application — the bindings,
/// here — and is handed to the daemon at construction; core carries
/// only the [`core::Registry`] container.
pub(crate) fn protocol_registry() -> core::Registry {
    core::Registry::new(vec![
        core::ProtocolInfo {
            name: "grbl",
            label: "GRBL",
            dialect_family: "grbl",
            connection: "serial_or_telnet_or_bluetooth",
            config_keys: &[
                "port", "baudrate", "host", "tcp_port", "address", "channel",
            ],
            methods: core::methods(GRBL_EXTRA),
            properties: core::buffer_properties(),
            probe: Some(core::ProbeFingerprint {
                command: "$I",
                signature: "VER:",
            }),
        },
        core::ProtocolInfo {
            name: "grbl_simple",
            label: "GRBL (simple)",
            dialect_family: "grbl",
            connection: "serial_or_telnet_or_bluetooth",
            config_keys: &[
                "port", "baudrate", "host", "tcp_port", "address", "channel",
            ],
            methods: core::methods(GRBL_EXTRA),
            properties: core::common_properties(),
            probe: None,
        },
        core::ProtocolInfo {
            name: "esp3d",
            label: "FluidNC / ESP3D",
            dialect_family: "grbl",
            connection: "network",
            config_keys: &["host", "http_port", "ws_port", "protocol"],
            methods: core::methods(GRBL_EXTRA),
            properties: core::common_properties(),
            probe: None,
        },
        core::ProtocolInfo {
            name: "smoothie",
            label: "Smoothieware",
            dialect_family: "smoothieware",
            connection: "telnet",
            config_keys: &["host", "tcp_port"],
            methods: core::methods(&[
                "set_hold",
                "clear_alarm",
                "probe_device",
            ]),
            properties: core::common_properties(),
            probe: None,
        },
        core::ProtocolInfo {
            name: "octoprint",
            label: "OctoPrint",
            dialect_family: "grbl",
            connection: "network",
            config_keys: &["host", "port", "path", "api_key"],
            // The session implements the uniform surface, but the
            // server API cannot back settings or unit claims and
            // returns empty results for them, so those methods are
            // not advertised as remote-callable.
            methods: core::methods(&[
                "clear_alarm",
                "probe_device",
                "probe_info",
                "read_wcs_offsets",
                "read_parser_state",
                "run_probe_cycle",
                "set_hold",
            ]),
            properties: core::common_properties(),
            probe: None,
        },
        core::ProtocolInfo {
            name: "marlin",
            label: "Marlin",
            dialect_family: "marlin",
            connection: "serial",
            config_keys: &["port", "baudrate"],
            methods: core::methods(&[
                "clear_alarm",
                "detect_unit_system",
                "probe_device",
            ]),
            properties: core::common_properties(),
            probe: Some(core::ProbeFingerprint {
                command: "M115",
                signature: "FIRMWARE",
            }),
        },
        core::ProtocolInfo {
            name: "epilog",
            label: "Epilog",
            // Epilog print jobs are PJL/PCL/HPGL, not G-code.
            dialect_family: "",
            connection: "network",
            config_keys: &["host", "port", "timeout"],
            // The machine has no status channel, so only the job
            // upload and the machine-space coordinate query apply.
            methods: core::methods(&[
                "upload",
                "read_wcs_offsets",
                "read_parser_state",
            ]),
            properties: core::common_properties(),
            probe: None,
        },
    ])
}

/// Look up a protocol's metadata, raising ``KeyError`` if unknown.
#[gen_stub_pyfunction(module = "raydriver.protocols")]
#[pyfunction]
fn protocol_info(name: &str) -> PyResult<ProtocolInfo> {
    protocol_registry()
        .protocol_info(name)
        .map(|inner| ProtocolInfo { inner })
        .ok_or_else(|| PyKeyError::new_err(name.to_owned()))
}

/// Every protocol's metadata, keyed by protocol name.
#[gen_stub_pyfunction(module = "raydriver.protocols")]
#[gen_stub(override_return_type(
    type_repr = "dict[builtins.str, ProtocolInfo]",
))]
#[pyfunction]
fn protocols(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let dict = PyDict::new(py);
    for info in protocol_registry().protocols() {
        let inner = info.clone();
        dict.set_item(info.name, Py::new(py, ProtocolInfo { inner })?)?;
    }
    Ok(dict)
}

/// The ``protocol/list`` payload: every protocol's metadata.
#[gen_stub_pyfunction(module = "raydriver.protocols")]
#[gen_stub(override_return_type(type_repr = "dict[builtins.str, builtins.object]", imports = ("builtins",)))]
#[pyfunction]
fn protocols_payload(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    Ok(value_to_py(py, &protocol_registry().protocols_payload())?
        .into_bound(py))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let module = PyModule::new(py, "protocols")?;
    module.setattr(
        "__doc__",
        "Protocol registry: identity, config shape and capability \
         claims for every hostable session protocol.",
    )?;
    module.add(
        "__all__",
        vec!["PROTOCOLS", "ProtocolInfo", "protocol_info", "protocols"],
    )?;
    // The registry as a name -> metadata mapping.
    let protocols_dict = protocols(py)?;
    module.setattr("PROTOCOLS", protocols_dict)?;
    module.add_class::<ProtocolInfo>()?;
    module.add_function(wrap_pyfunction!(protocol_info, &module)?)?;
    module.add_function(wrap_pyfunction!(protocols, &module)?)?;
    module.add_function(wrap_pyfunction!(protocols_payload, &module)?)?;
    crate::python::core::discovery::helpers::attach_submodule(
        m,
        &module,
        "raydriver.protocols",
    )?;
    Ok(())
}

pyo3_stub_gen::module_variable!(
    "raydriver.protocols",
    "PROTOCOLS",
    std::collections::HashMap<String, ProtocolInfo>
);
