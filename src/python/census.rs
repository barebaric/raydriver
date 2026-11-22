//! The protocol-generic device census surface.
//!
//! A session's ``probe_device`` resolves to the census pyclass of
//! its own protocol; the remote proxy resolves to whichever one the
//! hosted protocol produces, typed as their union.

use pyo3::prelude::*;
use pyo3_stub_gen::{ImportRef, PyStubType, TypeInfo};
use serde_json::Value;

use crate::core::remote::registry::CensusKind;
use crate::python::grbl::probe::GrblDeviceInfo;
use crate::python::marlin::probe::MarlinDeviceInfo;
use crate::python::octoprint::probe::OctoPrintDeviceInfo;
use crate::python::smoothie::probe::SmoothieDeviceInfo;

/// The census a session's ``probe_device`` returns: the concrete
/// type depends on the session's protocol.
pub(crate) struct DeviceCensus(Py<PyAny>);

impl<'py> IntoPyObject<'py> for DeviceCensus {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(self.0.into_bound(py))
    }
}

impl PyStubType for DeviceCensus {
    fn type_output() -> TypeInfo {
        // Qualified through each protocol's package, so the four
        // `probe` submodules cannot collide on one import name.
        TypeInfo {
            name: "grbl.probe.GrblDeviceInfo \
                   | marlin.probe.MarlinDeviceInfo \
                   | smoothie.probe.SmoothieDeviceInfo \
                   | octoprint.probe.OctoPrintDeviceInfo"
                .to_string(),
            source_module: None,
            import: [
                "raydriver.grbl",
                "raydriver.marlin",
                "raydriver.smoothie",
                "raydriver.octoprint",
            ]
            .into_iter()
            .map(ImportRef::from)
            .collect(),
            type_refs: Default::default(),
        }
    }
}

/// Builds the census pyclass for *census* from its wire form.
pub(crate) fn census_from_json(
    py: Python<'_>,
    census: CensusKind,
    value: &Value,
) -> PyResult<DeviceCensus> {
    let any: Py<PyAny> = match census {
        CensusKind::Grbl => {
            let census =
                crate::grbl::probe::GrblCensus::from_json(value.clone())
                    .map_err(|err| {
                        pyo3::exceptions::PyValueError::new_err(err.to_string())
                    })?;
            Py::new(py, GrblDeviceInfo { inner: census })?.into_any()
        }
        CensusKind::Marlin => {
            let census =
                crate::marlin::probe::MarlinCensus::from_json(value.clone())
                    .map_err(|err| {
                        pyo3::exceptions::PyValueError::new_err(err.to_string())
                    })?;
            Py::new(py, MarlinDeviceInfo { inner: census })?.into_any()
        }
        CensusKind::Smoothie => {
            let census = crate::smoothie::probe::SmoothieCensus::from_json(
                value.clone(),
            )
            .map_err(|err| {
                pyo3::exceptions::PyValueError::new_err(err.to_string())
            })?;
            Py::new(py, SmoothieDeviceInfo { inner: census })?.into_any()
        }
        CensusKind::OctoPrint => {
            let census = crate::octoprint::probe::OctoPrintCensus::from_json(
                value.clone(),
            )
            .map_err(|err| {
                pyo3::exceptions::PyValueError::new_err(err.to_string())
            })?;
            Py::new(py, OctoPrintDeviceInfo { inner: census })?.into_any()
        }
    };
    Ok(DeviceCensus(any))
}
