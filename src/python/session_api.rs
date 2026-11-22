//! The uniform Python session API, shared by every session class.
//!
//! Rayforge's `SessionLike` protocol
//! (`rayforge/machine/driver/session_driver.py`) is the contract this
//! crate implements: every session — local or remote proxy — exposes
//! exactly these methods with exactly these signatures, so drivers
//! need no per-protocol adaptation layer.  The macro below is the
//! single definition of that surface; each binding invokes it for its
//! class and keeps only its constructor, protocol-specific extras and
//! properties.
//!
//! Operations a protocol cannot perform raise
//! [`SessionError::Unsupported`](crate::core::session::SessionError)
//! through the usual mapping — the method is still present, so
//! callers never branch on `hasattr`.

/// Implements the uniform session surface for a pyclass with `core`
/// and `events` fields.
macro_rules! uniform_session_api {
    ($cls:ty) => {
        #[gen_stub_pymethods]
        #[pymethods]
        impl $cls {
            /// Open the connection and keep it up until `disconnect`.
            fn connect<'py>(&self, py: Python<'py>) -> PyResult<PyAwait<'py, ()>> {
                self.events.capture_loop(py)?;
                let core = self.core.clone();
                spawn_future(py, async move {
                    $crate::core::session::Session::connect(core).await?;
                    Ok(())
                })
            }

            /// Stop all background tasks and close the connection.
            fn disconnect<'py>(
                &self,
                py: Python<'py>,
            ) -> PyResult<PyAwait<'py, ()>> {
                let core = self.core.clone();
                let events = self.events.clone();
                spawn_future(py, async move {
                    $crate::core::session::Session::cleanup(&*core).await;
                    events.quiesce();
                    Ok(())
                })
            }

            /// Replace the command templates.
            fn update_dialect(
                &self,
                dialect: Bound<'_, PyDict>,
            ) -> PyResult<()> {
                self.core.update_dialect(
                    $crate::python::grbl::session::parse_dialect(Some(&dialect))?,
                );
                Ok(())
            }

            /// Send one interactive command and return its response
            /// lines.
            fn execute_interactive_command<'py>(
                &self,
                py: Python<'py>,
                command: String,
            ) -> PyResult<PyAwait<'py, Vec<String>>> {
                let core = self.core.clone();
                spawn_future(py, async move {
                    Ok($crate::core::session::Session::execute_interactive_command(
                        &*core, &command,
                    )
                    .await?)
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
                    Ok($crate::core::session::Session::execute_command(
                        &*core, &command,
                    )
                    .await?)
                })
            }

            /// Stream a G-code job.
            #[pyo3(signature = (gcode, op_line_map=None, op_estimates=None, progress_callback=None))]
            fn run<'py>(
                &self,
                py: Python<'py>,
                gcode: String,
                op_line_map: Option<std::collections::HashMap<usize, i64>>,
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
                spawn_future(py, async move {
                    events.set_progress_callback(progress_callback);
                    $crate::core::session::Session::run(
                        core,
                        &gcode,
                        &op_map,
                        &estimates,
                    )
                    .await?;
                    Ok(())
                })
            }

            /// Execute pre-rendered machine code.
            fn run_raw<'py>(
                &self,
                py: Python<'py>,
                machine_code: String,
            ) -> PyResult<PyAwait<'py, ()>> {
                let core = self.core.clone();
                spawn_future(py, async move {
                    $crate::core::session::Session::run_raw(core, &machine_code)
                        .await?;
                    Ok(())
                })
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
                    $crate::core::session::Session::cancel(&*core, mode).await?;
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
                    $crate::core::session::Session::set_hold(&*core, hold).await?;
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
                let active_wcs =
                    active_wcs.unwrap_or_else(|| "G54".to_string());
                spawn_future(py, async move {
                    $crate::core::session::Session::home(
                        &*core,
                        axes,
                        &active_wcs,
                    )
                    .await?;
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
                    $crate::core::session::Session::move_to(
                        &*core, speed, x, y, z,
                    )
                    .await?;
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
                    $crate::core::session::Session::jog(&*core, speed, &deltas)
                        .await?;
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
                    $crate::core::session::Session::select_tool(
                        &*core,
                        tool_number,
                    )
                    .await?;
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
                    $crate::core::session::Session::set_power(&*core, power)
                        .await?;
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
                    $crate::core::session::Session::set_focus_power(&*core, power)
                        .await?;
                    Ok(())
                })
            }

            /// Clear a device alarm.
            fn clear_alarm<'py>(&self, py: Python<'py>) -> PyResult<PyAwait<'py, ()>> {
                let core = self.core.clone();
                spawn_future(py, async move {
                    $crate::core::session::Session::clear_alarm(&*core).await?;
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
                    $crate::core::session::Session::set_wcs_offset(
                        &*core, &wcs_slot, x, y, z,
                    )
                    .await?;
                    Ok(())
                })
            }

            /// Read the device's firmware settings.
            fn read_settings<'py>(
                &self,
                py: Python<'py>,
            ) -> PyResult<PyAwait<'py, Vec<(String, String)>>> {
                let core = self.core.clone();
                spawn_future(py, async move {
                    Ok($crate::core::session::Session::read_settings(&*core)
                        .await?)
                })
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
                    $crate::core::session::Session::write_setting(
                        &*core, &key, &value,
                    )
                    .await?;
                    Ok(())
                })
            }

            /// Read the work-coordinate-system offsets.
            fn read_wcs_offsets<'py>(
                &self,
                py: Python<'py>,
            ) -> PyResult<PyAwait<'py, std::collections::HashMap<String, (f64, f64, f64)>>> {
                let core = self.core.clone();
                spawn_future(py, async move {
                    Ok($crate::core::session::Session::read_wcs_offsets(&*core)
                        .await?)
                })
            }

            /// Read the G-code parser state.
            fn read_parser_state<'py>(
                &self,
                py: Python<'py>,
            ) -> PyResult<PyAwait<'py, Option<String>>> {
                let core = self.core.clone();
                spawn_future(py, async move {
                    Ok($crate::core::session::Session::read_parser_state(&*core)
                        .await?)
                })
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
                    Ok($crate::core::session::Session::run_probe_cycle(
                        &*core,
                        &axis_letter,
                        max_travel,
                        feed_rate,
                    )
                    .await?)
                })
            }

            /// Detect the device's unit system (`"metric"` or
            /// `"imperial"`), or `None` when unknown.
            fn detect_unit_system<'py>(
                &self,
                py: Python<'py>,
            ) -> PyResult<PyAwait<'py, Option<String>>> {
                let core = self.core.clone();
                spawn_future(py, async move {
                    let unit =
                        $crate::core::session::Session::detect_unit_system(
                            &*core,
                        )
                        .await?;
                    Ok(unit.map(|unit| {
                        match unit {
                            $crate::core::types::UnitSystem::Metric => {
                                "metric"
                            }
                            $crate::core::types::UnitSystem::Imperial => {
                                "imperial"
                            }
                        }
                        .to_string()
                    }))
                })
            }
        }
    };
}

pub(crate) use uniform_session_api;
