//! The `raydriver.driver` PyO3 module: the `Driver` base class.
//!
//! Every protocol driver class is a real Python subclass of
//! [`PyDriver`] (PyO3 `extends`), so the uniform surface is declared
//! exactly once here, inherited through the MRO, and visible once in
//! the generated stubs.  The base dispatches to the driver object
//! stored as `Arc<dyn Driver>`; protocol subclasses keep their
//! concrete `Arc` for their extras.

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::core::driver::{CancelMode, Driver, DriverInfo};

pub(crate) mod info;
use crate::python::core::types::DeviceState;
use crate::python::events::PyEvents;
use crate::python::pyfuture::{
    spawn_future, spawn_future_with_cancel, PyAwait,
};

/// The base class of every raydriver driver.
///
/// Declares the uniform surface once: connection lifecycle, job
/// streaming, machine control, probing and settings.  Operations a
/// protocol cannot perform raise `NotImplementedError`; the method
/// is still present on every driver.
#[gen_stub_pyclass]
#[pyclass(name = "Driver", module = "raydriver.driver", subclass)]
pub struct PyDriver {
    pub(crate) core: Arc<dyn Driver>,
    pub(crate) events: Arc<PyEvents>,
}

impl PyDriver {
    /// Build the base layer for a concrete driver.
    #[allow(dead_code)]
    pub(crate) fn new(core: Arc<dyn Driver>, events: Arc<PyEvents>) -> Self {
        Self { core, events }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyDriver {
    /// Open the connection and keep it up until `disconnect`.
    fn connect<'py>(&self, py: Python<'py>) -> PyResult<PyAwait<'py, ()>> {
        self.events.capture_loop(py)?;
        let core = self.core.clone();
        spawn_future(py, async move {
            Driver::connect(core).await?;
            Ok(())
        })
    }

    /// Stop all background tasks and close the connection.
    fn disconnect<'py>(&self, py: Python<'py>) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        let events = self.events.clone();
        spawn_future(py, async move {
            Driver::cleanup(&*core).await;
            events.quiesce();
            Ok(())
        })
    }

    /// Replace the command templates.
    fn update_dialect(&self, dialect: Bound<'_, PyDict>) -> PyResult<()> {
        self.core
            .update_dialect(crate::python::grbl::driver::parse_dialect(Some(
                &dialect,
            ))?);
        Ok(())
    }

    /// Send one interactive command and return its response lines.
    fn execute_interactive_command<'py>(
        &self,
        py: Python<'py>,
        command: String,
    ) -> PyResult<PyAwait<'py, Vec<String>>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            Ok(Driver::execute_interactive_command(&*core, &command).await?)
        })
    }

    /// Send one command and return its response lines.
    fn execute_command<'py>(
        &self,
        py: Python<'py>,
        command: String,
    ) -> PyResult<PyAwait<'py, Vec<String>>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            Ok(Driver::execute_command(&*core, &command).await?)
        })
    }

    /// Stream a G-code job.
    #[pyo3(signature = (gcode, op_line_map=None, op_estimates=None, progress_callback=None))]
    fn run<'py>(
        &self,
        py: Python<'py>,
        gcode: String,
        op_line_map: Option<HashMap<usize, i64>>,
        op_estimates: Option<Vec<f64>>,
        #[gen_stub(override_type(
            type_repr = "typing.Optional[collections.abc.Callable[[builtins.int], builtins.object]]",
            imports = ("typing", "collections.abc")
        ))]
        progress_callback: Option<Py<PyAny>>,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        let events = self.events.clone();
        let op_map = op_line_map.unwrap_or_default();
        let estimates = op_estimates.unwrap_or_default();
        let cancel_core = core.clone();
        spawn_future_with_cancel(
            py,
            async move {
                events.set_progress_callback(progress_callback);
                Driver::run(core, &gcode, &op_map, &estimates).await?;
                Ok(())
            },
            move || {
                crate::python::runtime::handle().spawn(async move {
                    let _ = Driver::cancel(&*cancel_core, CancelMode::Graceful)
                        .await;
                });
            },
        )
    }

    /// Execute pre-rendered machine code.
    fn run_raw<'py>(
        &self,
        py: Python<'py>,
        machine_code: String,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        let cancel_core = core.clone();
        spawn_future_with_cancel(
            py,
            async move {
                Driver::run_raw(core, &machine_code).await?;
                Ok(())
            },
            move || {
                crate::python::runtime::handle().spawn(async move {
                    let _ = Driver::cancel(&*cancel_core, CancelMode::Graceful)
                        .await;
                });
            },
        )
    }

    /// Cancel the running job.
    #[pyo3(signature = (emergency=false))]
    fn cancel<'py>(
        &self,
        py: Python<'py>,
        emergency: bool,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            let mode = if emergency {
                CancelMode::Immediate
            } else {
                CancelMode::Graceful
            };
            Driver::cancel(&*core, mode).await?;
            Ok(())
        })
    }

    /// Pause or resume the running job.
    #[pyo3(signature = (hold=true))]
    fn set_hold<'py>(
        &self,
        py: Python<'py>,
        hold: bool,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            Driver::set_hold(&*core, hold).await?;
            Ok(())
        })
    }

    /// Home all axes or the given axes.
    #[pyo3(signature = (axes=None, active_wcs=None))]
    fn home<'py>(
        &self,
        py: Python<'py>,
        axes: Option<Vec<String>>,
        active_wcs: Option<String>,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        let active_wcs = active_wcs.unwrap_or_else(|| "G54".to_string());
        spawn_future(py, async move {
            Driver::home(&*core, axes, &active_wcs).await?;
            Ok(())
        })
    }

    /// Absolute move in the machine's current coordinate frame.
    #[pyo3(signature = (speed, x, y, z=None))]
    fn move_to<'py>(
        &self,
        py: Python<'py>,
        speed: f64,
        x: f64,
        y: f64,
        z: Option<f64>,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            Driver::move_to(&*core, speed, x, y, z).await?;
            Ok(())
        })
    }

    /// Relative jog by per-axis distances.
    #[pyo3(signature = (speed, deltas=None))]
    fn jog<'py>(
        &self,
        py: Python<'py>,
        speed: f64,
        deltas: Option<Vec<(String, f64)>>,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        let deltas = deltas.unwrap_or_default();
        spawn_future(py, async move {
            Driver::jog(&*core, speed, &deltas).await?;
            Ok(())
        })
    }

    /// Select a tool.
    fn select_tool<'py>(
        &self,
        py: Python<'py>,
        tool_number: i64,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            Driver::select_tool(&*core, tool_number).await?;
            Ok(())
        })
    }

    /// Set the laser/spindle power; `None` disables the tool.
    #[pyo3(signature = (power=None))]
    fn set_power<'py>(
        &self,
        py: Python<'py>,
        power: Option<f64>,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            Driver::set_power(&*core, power).await?;
            Ok(())
        })
    }

    /// Set the laser power used while focusing.
    #[pyo3(signature = (power=None))]
    fn set_focus_power<'py>(
        &self,
        py: Python<'py>,
        power: Option<f64>,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            Driver::set_focus_power(&*core, power).await?;
            Ok(())
        })
    }

    /// Clear a device alarm.
    fn clear_alarm<'py>(&self, py: Python<'py>) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            Driver::clear_alarm(&*core).await?;
            Ok(())
        })
    }

    /// Write a work-coordinate-system offset.
    #[pyo3(signature = (wcs_slot, x, y, z=None))]
    fn set_wcs_offset<'py>(
        &self,
        py: Python<'py>,
        wcs_slot: String,
        x: f64,
        y: f64,
        z: Option<f64>,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            Driver::set_wcs_offset(&*core, &wcs_slot, x, y, z).await?;
            Ok(())
        })
    }

    /// Read the device's firmware settings.
    fn read_settings<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<PyAwait<'py, Vec<(String, String)>>> {
        let core = self.core.clone();
        spawn_future(
            py,
            async move { Ok(Driver::read_settings(&*core).await?) },
        )
    }

    /// Write one firmware setting.
    fn write_setting<'py>(
        &self,
        py: Python<'py>,
        key: String,
        value: String,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            Driver::write_setting(&*core, &key, &value).await?;
            Ok(())
        })
    }

    /// Read the work-coordinate-system offsets.
    fn read_wcs_offsets<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<PyAwait<'py, crate::core::driver::WcsOffsets>> {
        let core = self.core.clone();
        spawn_future(
            py,
            async move { Ok(Driver::read_wcs_offsets(&*core).await?) },
        )
    }

    /// Read the G-code parser state.
    fn read_parser_state<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<PyAwait<'py, Option<String>>> {
        let core = self.core.clone();
        spawn_future(
            py,
            async move { Ok(Driver::read_parser_state(&*core).await?) },
        )
    }

    /// Run a probe cycle; `None` = no trigger reported.
    fn run_probe_cycle<'py>(
        &self,
        py: Python<'py>,
        axis_letter: String,
        max_travel: f64,
        feed_rate: f64,
    ) -> PyResult<PyAwait<'py, Option<(f64, f64, f64)>>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            Ok(Driver::run_probe_cycle(
                &*core,
                &axis_letter,
                max_travel,
                feed_rate,
            )
            .await?)
        })
    }

    /// Detect the device's unit system (`"metric"` or `"imperial"`),
    /// or `None` when unknown.
    fn detect_unit_system<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<PyAwait<'py, Option<String>>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            let unit = Driver::detect_unit_system(&*core).await?;
            Ok(unit.map(|unit| {
                match unit {
                    crate::core::types::UnitSystem::Metric => "metric",
                    crate::core::types::UnitSystem::Imperial => "imperial",
                }
                .to_string()
            }))
        })
    }

    /// The driver's typed identity and capabilities.
    #[getter]
    fn info(&self) -> crate::python::driver::info::DriverInfo {
        crate::python::driver::info::DriverInfo::new(self.core.info().clone())
    }

    /// Current snapshot of the device state.
    #[getter]
    fn state(&self) -> DeviceState {
        DeviceState::new(Driver::state(&*self.core))
    }

    /// True while a job is running or held.
    #[getter]
    fn job_running(&self) -> bool {
        Driver::is_job_running(&*self.core)
    }

    /// Bytes currently occupying the device's RX buffer.
    #[getter]
    fn buffer_count(&self) -> usize {
        Driver::buffer_count(&*self.core)
    }

    /// The device's RX buffer size in bytes, or 0 when unknown.
    #[getter]
    fn rx_buffer_size(&self) -> usize {
        Driver::rx_buffer_size(&*self.core)
    }

    /// Commands sent but not yet acknowledged.
    #[getter]
    fn pending_commands(&self) -> Vec<String> {
        Driver::pending_commands(&*self.core)
    }

    /// Identifier of the physical resource, or `None`.
    #[getter]
    fn resource_uri(&self) -> Option<String> {
        Driver::resource_uri(&*self.core)
    }
}

impl Drop for PyDriver {
    fn drop(&mut self) {
        self.events.close();
        self.core.abort_background_tasks_now();
    }
}

/// The natural move target for a host-held driver list.
#[allow(dead_code)]
pub(crate) fn info_of(driver: &dyn Driver) -> DriverInfo {
    driver.info().clone()
}

pub(crate) const MODULE_DOC: &str = "\
The common driver base class and its typed metadata.

Every protocol driver is a subclass of Driver: the uniform surface \
(connection lifecycle, job streaming, machine control, probing and \
settings) is declared here once, with strict types, and inherited by \
every driver class. Operations a protocol cannot perform raise \
NotImplementedError.
";

pyo3_stub_gen::module_doc!("raydriver.driver", "{}", MODULE_DOC);

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let module = PyModule::new(py, "driver")?;
    module.setattr("__doc__", MODULE_DOC)?;
    module.add("__all__", vec!["Driver", "DriverInfo"])?;
    module.add_class::<PyDriver>()?;
    module.add_class::<info::DriverInfo>()?;
    crate::python::core::discovery::helpers::attach_submodule(
        m,
        &module,
        "raydriver.driver",
    )?;
    Ok(())
}
