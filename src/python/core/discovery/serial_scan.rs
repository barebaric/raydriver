//! The `raydriver.core.discovery.serial_scan` submodule: serial port
//! scanning.
//!
//! The capture probe runs in the Rust core; this submodule selects
//! the ports, attaches their USB metadata and shapes the results.
//! Interpreting an observation — deciding which driver could talk to
//! the device — is the serial channel's concern.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3_stub_gen::derive::{
    gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods,
};

use crate::core::discovery::serial;
use crate::python::pyfuture::spawn_future;

use super::helpers;
use super::ports::SerialPortInfo;

/// Raw output captured from one port during a scan.
#[gen_stub_pyclass(module = "raydriver.core.discovery.serial_scan")]
#[pyclass(
    module = "raydriver.core.discovery.serial_scan",
    frozen,
    skip_from_py_object
)]
pub struct PortObservation {
    port: String,
    baud_rate: u32,
    data: Vec<u8>,
    info: Option<SerialPortInfo>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PortObservation {
    #[new]
    #[pyo3(signature = (port, baud_rate, data, info=None))]
    fn new(
        port: String,
        baud_rate: u32,
        data: Vec<u8>,
        info: Option<SerialPortInfo>,
    ) -> Self {
        Self {
            port,
            baud_rate,
            data,
            info,
        }
    }

    #[getter]
    pub(crate) fn port(&self) -> String {
        self.port.clone()
    }

    #[getter]
    pub(crate) fn baud_rate(&self) -> u32 {
        self.baud_rate
    }

    /// Everything the device sent while the port was open.
    #[getter]
    #[gen_stub(override_return_type(type_repr = "builtins.bytes", imports = ("builtins",)))]
    pub(crate) fn data<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.data)
    }

    /// USB metadata for the port, when available.
    #[getter]
    pub(crate) fn info(&self) -> Option<SerialPortInfo> {
        self.info.clone()
    }
}

/// Generic, firmware-agnostic test for "a device is talking on this
/// port": the capture contains at least one line of mostly-printable
/// ASCII. Output received at a wrong baud rate is typically framing
/// garbage and fails this test.
#[gen_stub_pyfunction(module = "raydriver.core.discovery.serial_scan")]
#[pyfunction]
fn looks_like_device_output(data: Vec<u8>) -> bool {
    serial::looks_like_device_output(&data)
}

/// Probes every *port* at *baud_rates*, all ports in parallel, and
/// returns one observation per port that produced device-like
/// output. Ports named in *exclude_ports* are skipped entirely (used
/// by callers that hold devices on those ports). Ports that are busy,
/// absent, or silent are skipped silently.
///
/// When *timeout* is given the scan stops waiting after that many
/// seconds: observations collected so far are still returned and
/// probes still in flight are cancelled, so a slow or silent adapter
/// can never push a responsive machine past the deadline. Results
/// keep the order of *ports*.
#[gen_stub_pyfunction(module = "raydriver.core.discovery.serial_scan")]
#[pyfunction]
#[pyo3(signature = (ports=None, baud_rates=None, exclude_ports=None, timeout=None))]
fn scan_serial_ports<'py>(
    py: Python<'py>,
    #[gen_stub(override_type(type_repr = "typing.Optional[typing.Iterable[builtins.str]]", imports = ("typing", "builtins")))]
    ports: Option<Bound<'_, PyAny>>,
    #[gen_stub(override_type(type_repr = "typing.Optional[typing.Iterable[builtins.int]]", imports = ("typing", "builtins")))]
    baud_rates: Option<Bound<'_, PyAny>>,
    #[gen_stub(override_type(type_repr = "typing.Optional[typing.Iterable[builtins.str]]", imports = ("typing", "builtins")))]
    exclude_ports: Option<Bound<'_, PyAny>>,
    timeout: Option<f64>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut port_list = match helpers::opt_strings(ports)? {
        Some(ports) => ports,
        None => {
            let usb: Vec<String> =
                helpers::attr(py, "ports", "list_usb_ports")?
                    .call0()?
                    .extract()?;
            helpers::attr(py, "ports", "sort_ports")?
                .call1((usb,))?
                .extract()?
        }
    };
    if let Some(excluded) = exclude_ports {
        let excluded: HashSet<String> = excluded
            .try_iter()?
            .map(|item| item?.extract::<String>())
            .collect::<PyResult<_>>()?;
        port_list.retain(|p| !excluded.contains(p));
    }
    let rates = helpers::opt_u32(baud_rates)?
        .unwrap_or_else(|| serial::BAUD_CANDIDATES.to_vec());
    if port_list.is_empty() {
        return spawn_future(py, async move {
            Python::attach(|py| {
                let empty: Vec<PortObservation> = Vec::new();
                empty.into_pyobject(py).map(|o| o.unbind().into_any())
            })
        });
    }

    let infos: Vec<SerialPortInfo> =
        helpers::attr(py, "serial_scan", "list_port_info")?
            .call0()?
            .extract()?;
    let info_map: HashMap<String, SerialPortInfo> = infos
        .into_iter()
        .map(|info| (info.inner.device.clone(), info))
        .collect();
    let deadline = timeout.map(Duration::from_secs_f64);
    spawn_future(py, async move {
        let observations =
            serial::scan_serial_ports(port_list, rates, deadline).await;
        Python::attach(|py| {
            let out: Vec<PortObservation> = observations
                .into_iter()
                .map(|obs| PortObservation {
                    info: info_map.get(&obs.port).cloned(),
                    port: obs.port,
                    baud_rate: obs.baud_rate,
                    data: obs.data,
                })
                .collect();
            out.into_pyobject(py).map(|o| o.unbind().into_any())
        })
    })
}

pub fn register(discovery_mod: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = discovery_mod.py();
    let module = PyModule::new(py, "serial_scan")?;
    module.setattr(
        "__doc__",
        "Serial port scanning: capture probe plus port selection and \
         USB metadata.",
    )?;
    module.setattr("BAUD_CANDIDATES", serial::BAUD_CANDIDATES.to_vec())?;
    // The metadata backend, replaceable as a module attribute.
    let ports_mod = helpers::submodule(discovery_mod, "ports")?;
    module.setattr("list_port_info", ports_mod.getattr("list_port_info")?)?;
    module.add_class::<PortObservation>()?;
    module
        .add_function(wrap_pyfunction!(looks_like_device_output, &module)?)?;
    module.add_function(wrap_pyfunction!(scan_serial_ports, &module)?)?;
    helpers::attach_submodule(
        discovery_mod,
        &module,
        "raydriver.core.discovery.serial_scan",
    )?;
    Ok(())
}
pyo3_stub_gen::module_variable!(
    "raydriver.core.discovery.serial_scan",
    "BAUD_CANDIDATES",
    Vec<u32>
);
pyo3_stub_gen::module_variable!(
    "raydriver.core.discovery.serial_scan",
    "list_port_info",
    Py<PyAny>
);
