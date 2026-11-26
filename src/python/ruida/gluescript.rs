//! The `GlueScript` PyO3 wrapper: the Ruida job authoring and
//! compilation API, mirroring the methods the encoder drives.

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::ruida::ruidadriver::rd_gluescript::GlueScript as CoreGlueScript;

/// The Ruida job authoring and compilation model.
///
/// Call the authoring methods to build a job, then `stage_gluescript()`
/// compiles it into rpascript. `stage_gluescript(lines)` also replays a
/// persisted transcript.
#[gen_stub_pyclass(module = "raydriver.ruida")]
#[pyclass(name = "GlueScript", module = "raydriver.ruida")]
pub struct PyGlueScript {
    core: CoreGlueScript,
}

fn runtime_error(error: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}

#[gen_stub_pymethods]
#[pymethods]
impl PyGlueScript {
    #[new]
    #[gen_stub(override_return_type(type_repr = "GlueScript"))]
    fn new() -> Self {
        Self {
            core: CoreGlueScript::new(),
        }
    }

    /// Reset all per-job state.
    fn new_gluescript(&mut self) {
        self.core.new_gluescript();
    }

    /// Append comment lines to the job header.
    fn comment(&mut self, comments: Vec<String>) {
        self.core.comment(&comments);
    }

    /// Declare a job.
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (label, ref_point="MACHINE", abs_xy=None, columns=1, rows=1, xstep=0.0, ystep=0.0))]
    fn declare_job(
        &mut self,
        label: String,
        ref_point: &str,
        abs_xy: Option<Vec<f64>>,
        columns: i64,
        rows: i64,
        xstep: f64,
        ystep: f64,
    ) -> PyResult<()> {
        let abs_xy = abs_xy.map(|values| [values[0], values[1]]);
        self.core
            .declare_job(&label, ref_point, abs_xy, columns, rows, xstep, ystep)
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Finalize the job.
    fn end_job(&mut self) -> PyResult<()> {
        self.core
            .end_job()
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Declare a layer.
    #[pyo3(signature = (label, color, mode="VECTOR", overscan="NONE", speed=100.0, frequency=20.0, min_power_1=8.0, max_power_1=70.0))]
    #[allow(clippy::too_many_arguments)]
    fn declare_layer(
        &mut self,
        label: String,
        color: String,
        mode: &str,
        overscan: &str,
        speed: f64,
        frequency: f64,
        min_power_1: f64,
        max_power_1: f64,
    ) -> PyResult<()> {
        self.core
            .declare_layer(
                &label,
                &color,
                mode,
                overscan,
                speed,
                frequency,
                min_power_1,
                max_power_1,
            )
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Set immediate power for an IMAGE/DEPTHMAP layer.
    fn power(&mut self, percent: Option<f64>) {
        self.core.power(percent);
    }

    /// Save a power ramp range for the next cut.
    fn power_range(
        &mut self,
        min_power: Option<f64>,
        max_power: Option<f64>,
    ) -> PyResult<()> {
        self.core
            .power_range(min_power, max_power)
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Save the cut speed for the next cut.
    fn cut_speed(&mut self, speed: f64) {
        self.core.cut_speed(speed);
    }

    /// Record a move speed (comment-only).
    fn move_speed(&mut self, speed: f64) {
        self.core.move_speed(speed);
    }

    /// Save the layer frequency for the next flush.
    fn frequency(&mut self, frequency: f64) {
        self.core.frequency(frequency);
    }

    /// Record the pulse width (comment-only).
    fn pwm(&mut self, duration: f64) {
        self.core.pwm(duration);
    }

    /// Select a laser head.
    fn select_laser(&mut self, laser: i64) {
        self.core.select_laser(laser);
    }

    /// Pierce at the current position for a number of milliseconds.
    fn pierce(&mut self, duration: f64) -> PyResult<()> {
        self.core.pierce(duration).map_err(runtime_error)
    }

    /// Add a per-segment cut delay in milliseconds.
    fn add_delay(&mut self, duration: f64) -> PyResult<()> {
        self.core.add_delay(duration).map_err(runtime_error)
    }

    /// Set the delay before the laser fires.
    fn laser_on_delay(&mut self, duration: f64) -> PyResult<()> {
        self.core.laser_on_delay(duration).map_err(runtime_error)
    }

    /// Set the delay after the laser stops.
    fn laser_off_delay(&mut self, duration: f64) -> PyResult<()> {
        self.core.laser_off_delay(duration).map_err(runtime_error)
    }

    /// Append raw rpascript commands at the current position.
    fn inline(&mut self, commands: Vec<String>) {
        self.core.inline(&commands);
    }

    /// Add raw rpascript lines to a layer's action block.
    fn add_layer_action(&mut self, layer: i64, lines: Vec<String>) {
        self.core.add_layer_action(layer.max(0) as usize, &lines);
    }

    /// Sync tracked position from controller replies.
    #[pyo3(signature = (x=None, y=None, z=None, u=None))]
    fn update_position(
        &mut self,
        x: Option<f64>,
        y: Option<f64>,
        z: Option<f64>,
        u: Option<f64>,
    ) {
        self.core.update_position(x, y, z, u);
    }

    /// Enable air assist.
    fn air_assist_on(&mut self) {
        self.core.air_assist_on();
    }

    /// Disable air assist.
    fn air_assist_off(&mut self) {
        self.core.air_assist_off();
    }

    /// Switch the layer mode mid-stream.
    fn set_mode(&mut self, mode: &str) -> PyResult<()> {
        self.core
            .set_mode(mode)
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Set the overscan for the current layer.
    fn set_overscan(&mut self, overscan: &str) -> PyResult<()> {
        self.core
            .set_overscan(overscan)
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Set the maximum cut speed used by power scaling.
    fn set_max_cut_speed(&mut self, speed: f64) -> PyResult<()> {
        self.core
            .set_max_cut_speed(speed)
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Set the laser's minimum firing power.
    fn set_power_floor(&mut self, floor: f64) -> PyResult<()> {
        self.core
            .set_power_floor(floor)
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Enable or disable effective-min power scaling.
    fn set_power_scaling_enabled(&mut self, enabled: bool) {
        self.core.set_power_scaling_enabled(enabled);
    }

    /// Move to an absolute XY position.
    fn move_xy_to(&mut self, x: f64, y: f64) {
        self.core.move_xy_to(x, y);
    }

    /// Move to an absolute X position.
    fn move_x_to(&mut self, x: f64) {
        self.core.move_x_to(x);
    }

    /// Move to an absolute Y position.
    fn move_y_to(&mut self, y: f64) {
        self.core.move_y_to(y);
    }

    /// Cut to an absolute XY position.
    fn cut_xy_to(&mut self, x: f64, y: f64) -> PyResult<()> {
        self.core.cut_xy_to(x, y).map_err(runtime_error)
    }

    /// Cut to an absolute X position.
    fn cut_x_to(&mut self, x: f64) -> PyResult<()> {
        self.core.cut_x_to(x).map_err(runtime_error)
    }

    /// Cut to an absolute Y position.
    fn cut_y_to(&mut self, y: f64) -> PyResult<()> {
        self.core.cut_y_to(y).map_err(runtime_error)
    }

    /// Compile the job, or replay a transcript and compile it.
    #[pyo3(signature = (gluescript=None, require_complete=true))]
    fn stage_gluescript(
        &mut self,
        gluescript: Option<Vec<String>>,
        require_complete: bool,
    ) -> PyResult<String> {
        self.core
            .stage_gluescript(gluescript.as_deref(), require_complete)
            .map_err(runtime_error)
    }

    /// Replay only appended transcript lines and compile.
    #[pyo3(signature = (flushed_count, delta_lines, require_complete=true))]
    fn stage_gluescript_delta(
        &mut self,
        flushed_count: usize,
        delta_lines: Vec<String>,
        require_complete: bool,
    ) -> PyResult<String> {
        self.core
            .stage_gluescript_delta(
                flushed_count,
                &delta_lines,
                require_complete,
            )
            .map_err(runtime_error)
    }

    /// The canonical transcript lines.
    #[getter]
    fn gluescript(&self) -> Vec<String> {
        self.core.gluescript.clone()
    }

    /// The compiled rpascript lines.
    #[getter]
    fn rpascript(&self) -> Vec<String> {
        self.core.rpascript.clone()
    }

    /// Whether `end_job()` finalized the job.
    #[getter]
    fn job_complete(&self) -> bool {
        self.core.job_complete()
    }
}
