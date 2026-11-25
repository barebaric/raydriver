//! Python bindings for the machine-code encoders (`raydriver.encode`).
//!
//! Each spec class exposes ``__raygeo_encoder__()``, the cross-extension
//! handoff consumed by `raygeo.ops.convert.extract_encoder`: it returns
//! a named capsule carrying an owned `Box<dyn Encoder>`. The capsule
//! name embeds the `raygeo` version the encoder was built against, so a
//! mismatched pairing is rejected by raygeo instead of being
//! misinterpreted.
//!
//! The spec data itself stays plain: no raygeoPython types are needed
//! here beyond the `Encoder` trait, which is why the two crates' PyO3
//! versions stay independent.

use std::ffi::CString;

use pyo3::prelude::*;
use pyo3::types::PyCapsule;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::encode::epilog::EpilogSpec;
use crate::encode::gcode::GcodeSpec;
use crate::encode::gcode_types::GcodeDialectSpec;
use raygeo::ops::convert::{encoder_capsule_name, Encoder};

/// Build the capsule that hands an encoder to raygeo.
///
/// The capsule owns the boxed encoder and drops it when collected;
/// raygeo clones the encoder out of it.
fn encoder_capsule<'py>(
    py: Python<'py>,
    encoder: Box<dyn Encoder>,
) -> PyResult<Bound<'py, PyCapsule>> {
    let name = CString::new(encoder_capsule_name()).map_err(|_| {
        pyo3::exceptions::PyRuntimeError::new_err(
            "invalid raygeo encoder capsule name",
        )
    })?;
    PyCapsule::new(py, encoder, Some(name))
}

/// Typed Python wrapper around the Rust ``GcodeDialectSpec``.
///
/// Constructed in Python with keyword arguments; the inner Rust
/// struct is passed directly to the encoder without a serde round-trip.
#[gen_stub_pyclass(module = "raydriver.encode")]
#[pyclass(
    module = "raydriver.encode",
    name = "GcodeDialectSpec",
    frozen,
    skip_from_py_object
)]
pub struct PyGcodeDialectSpec(pub GcodeDialectSpec);

#[gen_stub_pymethods]
#[pymethods]
impl PyGcodeDialectSpec {
    #[new]
    #[pyo3(signature = (
        laser_on = "M4 S{power:.0f}".to_string(),
        laser_off = "M5".to_string(),
        tool_change = "T{tool_number}".to_string(),
        set_speed = "G1 F{speed:.0f}".to_string(),
        travel_move = "G0{x_cmd}{y_cmd}{z_cmd}{extra_cmd}{f_command}{s_command}".to_string(),
        linear_move = "G1{x_cmd}{y_cmd}{z_cmd}{extra_cmd}{f_command}{s_command}".to_string(),
        arc_cw = "G2{x_cmd}{y_cmd}{z_cmd} I{i} J{j}{extra_cmd}{f_command}{s_command}".to_string(),
        arc_ccw = "G3{x_cmd}{y_cmd}{z_cmd} I{i} J{j}{extra_cmd}{f_command}{s_command}".to_string(),
        air_assist_on = "M8".to_string(),
        air_assist_off = "M9".to_string(),
        bezier_cubic = String::new(),
        spindle_on_cw = "M3 S{rpm}".to_string(),
        spindle_on_ccw = "M4 S{rpm}".to_string(),
        spindle_off = "M5".to_string(),
        coolant_flood = "M8".to_string(),
        coolant_mist = "M7".to_string(),
        coolant_off = "M9".to_string(),
        dwell = "G4 P{seconds:.3f}".to_string(),
        preamble = vec!["G90".to_string()],
        postscript = vec!["M30".to_string()],
        inject_wcs_after_preamble = false,
        can_g0_with_speed = false,
        omit_unchanged_coords = true,
        continuous_laser_mode = false,
        modal_feedrate = false,
        gcode_precision = 3u8,
        focus_laser_on = String::new(),
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        laser_on: String,
        laser_off: String,
        tool_change: String,
        set_speed: String,
        travel_move: String,
        linear_move: String,
        arc_cw: String,
        arc_ccw: String,
        air_assist_on: String,
        air_assist_off: String,
        bezier_cubic: String,
        spindle_on_cw: String,
        spindle_on_ccw: String,
        spindle_off: String,
        coolant_flood: String,
        coolant_mist: String,
        coolant_off: String,
        dwell: String,
        preamble: Vec<String>,
        postscript: Vec<String>,
        inject_wcs_after_preamble: bool,
        can_g0_with_speed: bool,
        omit_unchanged_coords: bool,
        continuous_laser_mode: bool,
        modal_feedrate: bool,
        gcode_precision: u8,
        focus_laser_on: String,
    ) -> Self {
        PyGcodeDialectSpec(GcodeDialectSpec {
            laser_on,
            laser_off,
            focus_laser_on,
            tool_change,
            set_speed,
            travel_move,
            linear_move,
            arc_cw,
            arc_ccw,
            bezier_cubic,
            air_assist_on,
            air_assist_off,
            spindle_on_cw,
            spindle_on_ccw,
            spindle_off,
            coolant_flood,
            coolant_mist,
            coolant_off,
            dwell,
            preamble,
            postscript,
            inject_wcs_after_preamble,
            can_g0_with_speed,
            omit_unchanged_coords,
            continuous_laser_mode,
            modal_feedrate,
            gcode_precision,
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "GcodeDialectSpec(laser_on={:?}, laser_off={:?}, ...)",
            self.0.laser_on, self.0.laser_off,
        )
    }
}

/// Parameters for the G-code encoder.
#[gen_stub_pyclass(module = "raydriver.encode")]
#[pyclass(
    name = "GcodeSpec",
    module = "raydriver.encode",
    frozen,
    skip_from_py_object
)]
#[derive(Clone)]
pub struct PyGcodeSpec {
    #[pyo3(get)]
    pub dialect: Py<PyGcodeDialectSpec>,
    #[pyo3(get)]
    pub context_json: String,
}

impl PyGcodeSpec {
    fn to_core(&self, py: Python<'_>) -> GcodeSpec {
        GcodeSpec {
            dialect: self.dialect.borrow(py).0.clone(),
            context_json: self.context_json.clone(),
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyGcodeSpec {
    #[new]
    fn new(dialect: Py<PyGcodeDialectSpec>, context_json: String) -> Self {
        PyGcodeSpec {
            dialect,
            context_json,
        }
    }

    /// Hand this encoder to raygeo (capsule protocol).
    #[gen_stub(skip)]
    fn __raygeo_encoder__<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyCapsule>> {
        let encoder: Box<dyn Encoder> = Box::new(self.to_core(py));
        encoder_capsule(py, encoder)
    }
}

/// Parameters for the Epilog print-job encoder.
///
/// `context_json` is a JSON-serialised ``EpilogContext``: job title,
/// resolution, frequency, bed size, speed reference, origin layout and
/// the rotary flag.
#[gen_stub_pyclass(module = "raydriver.encode")]
#[pyclass(
    name = "EpilogSpec",
    module = "raydriver.encode",
    frozen,
    skip_from_py_object
)]
#[derive(Clone)]
pub struct PyEpilogSpec {
    #[pyo3(get)]
    pub context_json: String,
}

impl PyEpilogSpec {
    fn to_core(&self) -> EpilogSpec {
        EpilogSpec {
            context_json: self.context_json.clone(),
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyEpilogSpec {
    #[new]
    fn new(context_json: String) -> Self {
        PyEpilogSpec { context_json }
    }

    /// Hand this encoder to raygeo (capsule protocol).
    #[gen_stub(skip)]
    fn __raygeo_encoder__<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyCapsule>> {
        let encoder: Box<dyn Encoder> = Box::new(self.to_core());
        encoder_capsule(py, encoder)
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let module = PyModule::new(py, "encode")?;
    module.setattr(
        "__doc__",
        "Machine-code encoders: G-code dialects and device print jobs. \
         Specs are handed to raygeo's pipeline through a capsule \
         protocol.",
    )?;
    module.add(
        "__all__",
        vec!["EpilogSpec", "GcodeDialectSpec", "GcodeSpec"],
    )?;
    module.add_class::<PyGcodeDialectSpec>()?;
    module.add_class::<PyGcodeSpec>()?;
    module.add_class::<PyEpilogSpec>()?;
    crate::python::core::discovery::helpers::attach_submodule(
        m,
        &module,
        "raydriver.encode",
    )?;
    Ok(())
}
