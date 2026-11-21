//! The `MarlinSession` PyO3 wrapper.

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::core::dialect::Dialect;
use crate::core::transport::TransportKind;
use crate::marlin::session::{MarlinConfig, MarlinCore};
use crate::python::core::types::DeviceState;
use crate::python::events::PyEvents;
use crate::python::grbl::session::{
    get_bool, get_f64, parse_dialect, transport_kind_from_config,
};
use crate::python::marlin::probe::MarlinDeviceInfo;
use crate::python::pyfuture::{spawn_future, PyAwait};

fn build_marlin_config(
    kind: TransportKind,
    dict: Option<&Bound<'_, PyDict>>,
) -> PyResult<MarlinConfig> {
    let mut config = MarlinConfig::new(kind);
    let Some(dict) = dict else {
        return Ok(config);
    };
    if let Some(v) = get_bool(dict, "poll_status_while_running")? {
        config.poll_status_while_running = v;
    }
    if let Some(v) = get_f64(dict, "command_timeout")? {
        config.command_timeout = v;
    }
    if let Some(v) = get_f64(dict, "handshake_timeout")? {
        config.handshake_timeout = v;
    }
    if let Some(v) = get_f64(dict, "handshake_poll_interval")? {
        config.handshake_poll_interval = v;
    }
    if let Some(v) = get_f64(dict, "status_poll_interval")? {
        config.status_poll_interval = v;
    }
    if let Some(v) = get_f64(dict, "reconnect_delay")? {
        config.reconnect_delay = v;
    }
    if let Some(v) = get_f64(dict, "connect_settle")? {
        config.connect_settle = v;
    }
    Ok(config)
}

/// A Marlin device session using ping-pong communication.
///
/// Sends one command at a time and waits for `ok` before
/// proceeding; the device announces itself with a `start` line and
/// positions are polled with `M114`.  Marlin has no feed-hold
/// support — pausing is the caller's concern.
///
/// All async methods return asyncio futures; a typed `SessionEvent`
/// is delivered to `event_callback(event)` on the event loop thread.
#[gen_stub_pyclass]
#[pyclass(module = "raydriver.marlin")]
pub struct MarlinSession {
    core: Arc<MarlinCore>,
    events: Arc<PyEvents>,
}

#[gen_stub_pymethods]
#[pymethods]
impl MarlinSession {
    #[new]
    #[pyo3(signature = (config, dialect=None, event_callback=None))]
    fn new(
        #[gen_stub(override_type(
            type_repr = "typing.Optional[typing.Union[dict, config.MarlinConfig]]",
            imports = ("typing", "raydriver.config")
        ))]
        config: Option<Bound<'_, PyAny>>,

        dialect: Option<Bound<'_, PyDict>>,
        #[gen_stub(override_type(
            type_repr = "typing.Optional[collections.abc.Callable[[core.SessionEvent], builtins.object]]",
            imports = ("builtins", "collections.abc", "typing", "raydriver.core")
        ))]
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let config = crate::python::config::config_dict(config.as_ref())?;
        let kind = transport_kind_from_config(config.as_ref())?;
        let session_config = build_marlin_config(kind, config.as_ref())?;
        Self::build(session_config, dialect, event_callback)
    }

    /// Build a session around a [`MockTransport`] (for tests).
    #[staticmethod]
    #[pyo3(signature = (transport, config=None, dialect=None, event_callback=None))]
    fn with_transport(
        transport: PyRef<'_, crate::python::core::mock::MockTransport>,
        #[gen_stub(override_type(
            type_repr = "typing.Optional[typing.Union[dict, config.MarlinConfig]]",
            imports = ("typing", "raydriver.config")
        ))]
        config: Option<Bound<'_, PyAny>>,

        dialect: Option<Bound<'_, PyDict>>,
        #[gen_stub(override_type(
            type_repr = "typing.Optional[collections.abc.Callable[[core.SessionEvent], builtins.object]]",
            imports = ("builtins", "collections.abc", "typing", "raydriver.core")
        ))]
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let config = crate::python::config::config_dict(config.as_ref())?;
        let kind = TransportKind::Mock(transport.inner.clone());
        let session_config = build_marlin_config(kind, config.as_ref())?;
        Self::build(session_config, dialect, event_callback)
    }

    /// Open the connection and start the background tasks.
    /// Run the device census: query M115, M211, M503 and M149,
    /// and return the structured result as a dict. *boot_lines* are
    /// the startup messages the caller observed on the session,
    /// used to refine the firmware version.
    #[pyo3(signature = (boot_lines=None))]
    fn probe_device<'py>(
        &self,
        py: Python<'py>,
        boot_lines: Option<Vec<String>>,
    ) -> PyResult<PyAwait<'py, MarlinDeviceInfo>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            let mut replies: Vec<Vec<String>> = Vec::new();
            for command in crate::marlin::probe::CENSUS_COMMANDS {
                let lines = core
                    .execute_interactive_command(command.to_string())
                    .await?;
                replies.push(crate::marlin::probe::filter_ok_lines(&lines));
            }
            Ok(MarlinDeviceInfo {
                inner: crate::marlin::probe::describe(
                    &replies[0],
                    &replies[1],
                    &replies[2],
                    &replies[3],
                    &boot_lines.unwrap_or_default(),
                ),
            })
        })
    }

    fn connect<'py>(&self, py: Python<'py>) -> PyResult<PyAwait<'py, ()>> {
        self.events.capture_loop(py)?;
        let core = self.core.clone();
        spawn_future(py, async move {
            core.connect().await;
            Ok(())
        })
    }

    /// Stop all background tasks and close the transport.
    fn disconnect<'py>(&self, py: Python<'py>) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        let events = self.events.clone();
        spawn_future(py, async move {
            core.cleanup().await;
            events.quiesce();
            Ok(())
        })
    }

    /// Send a command and await its full response.
    fn execute_interactive_command<'py>(
        &self,
        py: Python<'py>,
        command: String,
    ) -> PyResult<PyAwait<'py, Vec<String>>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            Ok(core.execute_interactive_command(command).await?)
        })
    }

    /// Queue a command and await its response (identical to
    /// `execute_interactive_command`).
    fn execute_command<'py>(
        &self,
        py: Python<'py>,
        command: String,
    ) -> PyResult<PyAwait<'py, Vec<String>>> {
        let core = self.core.clone();
        spawn_future(
            py,
            async move { Ok(core.execute_command(command).await?) },
        )
    }

    /// Stream a G-code program using the ping-pong protocol.
    ///
    /// `op_line_map` maps G-code line indices to op indices for
    /// progress reporting.
    #[pyo3(signature = (gcode, op_line_map=None, progress_callback=None))]
    fn run<'py>(
        &self,
        py: Python<'py>,
        gcode: String,
        op_line_map: Option<HashMap<usize, i64>>,
        #[gen_stub(override_type(
            type_repr = "typing.Optional[collections.abc.Callable[[builtins.int], builtins.object]]",
            imports = ("typing", "collections.abc")
        ))]
        progress_callback: Option<Py<PyAny>>,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        let events = self.events.clone();
        let op_map = op_line_map.unwrap_or_default();
        spawn_future(py, async move {
            events.set_progress_callback(progress_callback);
            core.run(&gcode, op_map).await;
            Ok(())
        })
    }

    /// Execute a raw G-code string line by line.
    fn run_raw<'py>(
        &self,
        py: Python<'py>,
        machine_code: String,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.run_raw(&machine_code).await;
            Ok(())
        })
    }

    /// Abort the running job: M410 quick stop and safety shutdown.
    #[pyo3(signature = (emergency=false))]
    fn cancel<'py>(
        &self,
        py: Python<'py>,
        emergency: bool,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.cancel(emergency).await?;
            Ok(())
        })
    }

    /// Home all axes (`axes` is None) or the named axes (e.g.
    /// `["X"]`).
    #[pyo3(signature = (axes=None))]
    fn home<'py>(
        &self,
        py: Python<'py>,
        axes: Option<Vec<String>>,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.home(axes).await?;
            Ok(())
        })
    }

    /// Move to an absolute position (mm) at `speed` (mm/min).
    fn move_to<'py>(
        &self,
        py: Python<'py>,
        speed: f64,
        x: f64,
        y: f64,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.move_to(speed, x, y).await?;
            Ok(())
        })
    }

    /// Jog by relative deltas (mm) at `speed` (mm/min).
    fn jog<'py>(
        &self,
        py: Python<'py>,
        speed: f64,
        deltas: Vec<(String, f64)>,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.jog(speed, &deltas).await?;
            Ok(())
        })
    }

    fn select_tool<'py>(
        &self,
        py: Python<'py>,
        tool_number: i64,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.select_tool(tool_number).await?;
            Ok(())
        })
    }

    /// Clear an alarm state with the dialect's clear command.
    fn clear_alarm<'py>(&self, py: Python<'py>) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.clear_alarm().await?;
            Ok(())
        })
    }

    /// Set laser power.  `power` is the absolute S value; `None`
    /// (or zero) turns the laser off.
    #[pyo3(signature = (power=None))]
    fn set_power<'py>(
        &self,
        py: Python<'py>,
        power: Option<f64>,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.set_power(power).await?;
            Ok(())
        })
    }

    /// Set laser power for focus mode; `None` turns the laser off.
    #[pyo3(signature = (power=None))]
    fn set_focus_power<'py>(
        &self,
        py: Python<'py>,
        power: Option<f64>,
    ) -> PyResult<PyAwait<'py, ()>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            core.set_focus_power(power).await?;
            Ok(())
        })
    }

    /// Set a WCS offset via the dialect's `set_wcs_offset` template.
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
            core.set_wcs_offset(&wcs_slot, x, y, z).await?;
            Ok(())
        })
    }

    /// Query the device's active linear unit via `M149`
    /// ("metric"/"imperial"), or None when it cannot be determined.
    fn detect_unit_system<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<PyAwait<'py, Option<String>>> {
        let core = self.core.clone();
        spawn_future(py, async move {
            let unit = core.detect_unit_system().await?;
            Ok(unit.map(|u| match u {
                crate::core::types::UnitSystem::Metric => "metric".to_string(),
                crate::core::types::UnitSystem::Imperial => {
                    "imperial".to_string()
                }
            }))
        })
    }

    /// Swap the dialect command templates at runtime.
    fn update_dialect(&self, dialect: Bound<'_, PyDict>) -> PyResult<()> {
        self.core.update_dialect(parse_dialect(Some(&dialect))?);
        Ok(())
    }

    /// Current snapshot of the device state.
    #[getter]
    fn state(&self) -> DeviceState {
        DeviceState::new(self.core.state())
    }

    #[getter]
    fn job_running(&self) -> bool {
        self.core.is_job_running()
    }

    #[getter]
    fn resource_uri(&self) -> Option<String> {
        self.core.resource_uri()
    }

    fn __repr__(&self) -> String {
        format!(
            "MarlinSession(resource_uri={:?}, job_running={})",
            self.resource_uri(),
            self.job_running()
        )
    }
}

impl MarlinSession {
    fn build(
        config: MarlinConfig,
        dialect: Option<Bound<'_, PyDict>>,
        event_callback: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let dialect: Dialect = parse_dialect(dialect.as_ref())?;
        let events = Arc::new(PyEvents::new(event_callback));
        let core = MarlinCore::new(config, dialect, events.clone());
        Ok(Self { core, events })
    }
}

impl Drop for MarlinSession {
    fn drop(&mut self) {
        // Stop the background tasks even when the session is dropped
        // without disconnect(); otherwise they orphan on the shared
        // runtime and marshal events onto a possibly-closed loop.
        self.events.close();
        self.core.abort_background_tasks_now();
    }
}
