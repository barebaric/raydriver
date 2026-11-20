//! Python bindings for the remote client proxy.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pyo3::exceptions::{PyConnectionError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use pyo3_stub_gen::derive::{
    gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods,
};
use serde_json::{json, Value};

use crate::core::remote::client::{
    self as core, RemoteEndpoint, RemoteEvents,
    RemoteSession as CoreRemoteSession,
};
use crate::core::remote::protocol::WireEvent;
use crate::python::core::json::value_to_py;
use crate::python::core::types::DeviceState as PyDeviceState;
use crate::python::pyfuture::spawn_future;
use crate::python::remote::bridge::{py_to_json, running_loop};
use crate::python::remote::events::wire_to_py_event;
use crate::python::remote::session::PySessionFactory;

const DEFAULT_PORT: u16 = 8765;

pub(crate) struct PyRemoteEvents {
    event_callback: Option<Py<PyAny>>,
    progress_callback: Mutex<Option<Py<PyAny>>>,
    loop_: Mutex<Option<Py<PyAny>>>,
}

impl PyRemoteEvents {
    fn new(
        event_callback: Option<Py<PyAny>>,
        loop_: Option<Py<PyAny>>,
    ) -> Self {
        Self {
            event_callback,
            progress_callback: Mutex::new(None),
            loop_: Mutex::new(loop_),
        }
    }

    fn set_progress_callback(&self, callback: Option<Py<PyAny>>) {
        *self.progress_callback.lock().unwrap() = callback;
    }

    fn capture_loop(&self, py: Python<'_>) -> PyResult<()> {
        let mut slot = self.loop_.lock().unwrap();
        if slot.is_none() {
            *slot = Some(running_loop(py)?);
        }
        Ok(())
    }
}

impl RemoteEvents for PyRemoteEvents {
    fn event(&self, event: WireEvent) {
        let Some(callback) = &self.event_callback else {
            return;
        };
        Python::attach(|py| {
            let Ok(Some(event)) = wire_to_py_event(py, &event) else {
                return;
            };
            let loop_ = self
                .loop_
                .lock()
                .unwrap()
                .as_ref()
                .map(|loop_| loop_.clone_ref(py));
            let Some(loop_) = loop_ else {
                return;
            };
            let _ = loop_.bind(py).call_method1(
                "call_soon_threadsafe",
                (callback.clone_ref(py), event),
            );
        });
    }

    fn progress(&self, op: i64) {
        Python::attach(|py| {
            let callback = self
                .progress_callback
                .lock()
                .unwrap()
                .as_ref()
                .map(|callback| callback.clone_ref(py));
            let Some(callback) = callback else {
                return;
            };
            let loop_ = self
                .loop_
                .lock()
                .unwrap()
                .as_ref()
                .map(|loop_| loop_.clone_ref(py));
            let Some(loop_) = loop_ else {
                return;
            };
            let _ = loop_
                .bind(py)
                .call_method1("call_soon_threadsafe", (callback, op));
        });
    }
}

/// The proxy with the union of the sessions' method surfaces.
#[gen_stub_pyclass]
#[pyclass(module = "raydriver.remote.client")]
pub struct RemoteSession {
    inner: Arc<CoreRemoteSession>,
    events: Arc<PyRemoteEvents>,
}

#[gen_stub_pymethods]
#[pymethods]
impl RemoteSession {
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        protocol_name,
        config,
        dialect,
        event_callback,
        endpoint,
        device = None,
        session_id = None,
    ))]
    fn new(
        py: Python<'_>,
        protocol_name: Option<String>,
        config: Py<PyAny>,
        dialect: Option<Py<PyAny>>,
        event_callback: Option<Py<PyAny>>,
        endpoint: Py<PyAny>,
        device: Option<String>,
        session_id: Option<String>,
    ) -> PyResult<Self> {
        let config = config_to_value(py, &config)?;
        let dialect = dialect
            .map(|dialect| config_to_value(py, &dialect))
            .transpose()?;
        let endpoint = parse_endpoint(py, &endpoint)?;
        let loop_ = running_loop(py).ok();
        let events = Arc::new(PyRemoteEvents::new(event_callback, loop_));
        let inner = CoreRemoteSession::new(
            protocol_name,
            config,
            dialect,
            endpoint,
            device,
            session_id,
            events.clone(),
        );
        Ok(Self { inner, events })
    }

    #[getter]
    fn protocol(&self) -> Option<String> {
        self.inner.protocol()
    }

    #[getter]
    fn metadata(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self.inner.metadata() {
            Some(metadata) => value_to_py(py, &metadata),
            None => Ok(py.None()),
        }
    }

    #[getter]
    fn state(&self) -> PyDeviceState {
        PyDeviceState::new(self.inner.state())
    }

    #[getter]
    fn job_running(&self) -> bool {
        self.inner.job_running()
    }

    #[getter]
    fn resource_uri(&self) -> Option<String> {
        self.inner.resource_uri()
    }

    #[getter]
    fn buffer_count(&self) -> i64 {
        self.inner.buffer_count()
    }

    #[getter]
    fn rx_buffer_size(&self) -> i64 {
        self.inner.rx_buffer_size()
    }

    #[getter]
    fn pending_commands(&self) -> Vec<String> {
        self.inner.pending_commands()
    }

    #[getter]
    fn _endpoint(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let endpoint = self.inner.endpoint();
        value_to_py(
            py,
            &json!({
                "remote_host": endpoint.host,
                "remote_port": endpoint.port,
                "token": endpoint.token,
                "tls_ca": endpoint.tls_ca,
                "tls_hostname": endpoint.tls_hostname,
            }),
        )
    }

    #[pyo3(signature = (message = None))]
    fn drop_connection(&self, message: Option<String>) {
        self.inner
            .force_disconnect(message.as_deref().unwrap_or("connection lost"));
    }

    fn connect<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.events.capture_loop(py)?;
        let inner = self.inner.clone();
        spawn_future(py, async move {
            inner.connect().await.map_err(connection)?;
            Ok(Python::attach(|py| py.None()))
        })
    }

    fn disconnect<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            inner.disconnect().await.map_err(connection)?;
            Ok(Python::attach(|py| py.None()))
        })
    }

    fn attach<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.events.capture_loop(py)?;
        let inner = self.inner.clone();
        spawn_future(py, async move {
            inner.attach().await.map_err(connection)?;
            Ok(Python::attach(|py| py.None()))
        })
    }

    fn aclose<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            inner.aclose().await;
            Ok(Python::attach(|py| py.None()))
        })
    }

    fn execute_interactive_command<'py>(
        &self,
        py: Python<'py>,
        command: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            let value = inner
                .execute_interactive_command(&command)
                .await
                .map_err(connection)?;
            Python::attach(|py| value_to_py(py, &value))
        })
    }

    fn execute_command<'py>(
        &self,
        py: Python<'py>,
        command: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            let value =
                inner.execute_command(&command).await.map_err(connection)?;
            Python::attach(|py| value_to_py(py, &value))
        })
    }

    #[pyo3(signature = (
        gcode,
        op_line_map = None,
        op_estimates = None,
        progress_callback = None,
    ))]
    fn run<'py>(
        &self,
        py: Python<'py>,
        gcode: String,
        op_line_map: Option<Py<PyAny>>,
        op_estimates: Option<Vec<f64>>,
        progress_callback: Option<Py<PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.events.capture_loop(py)?;
        let op_map = op_line_map
            .map(|op_map| op_map_to_map(py, &op_map))
            .transpose()?;
        self.events.set_progress_callback(progress_callback);
        let inner = self.inner.clone();
        let events = self.events.clone();
        spawn_future(py, async move {
            let result = inner.run(&gcode, op_map, op_estimates).await;
            events.set_progress_callback(None);
            result.map_err(connection)?;
            Ok(Python::attach(|py| py.None()))
        })
    }

    fn run_raw<'py>(
        &self,
        py: Python<'py>,
        machine_code: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            inner.run_raw(&machine_code).await.map_err(connection)?;
            Ok(Python::attach(|py| py.None()))
        })
    }

    #[pyo3(signature = (hold = true))]
    fn set_hold<'py>(
        &self,
        py: Python<'py>,
        hold: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            inner.set_hold(hold).await.map_err(connection)?;
            Ok(Python::attach(|py| py.None()))
        })
    }

    #[pyo3(signature = (emergency = false))]
    fn cancel<'py>(
        &self,
        py: Python<'py>,
        emergency: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            inner.cancel(emergency).await.map_err(connection)?;
            Ok(Python::attach(|py| py.None()))
        })
    }

    #[pyo3(signature = (axes = None, active_wcs = None))]
    fn home<'py>(
        &self,
        py: Python<'py>,
        axes: Option<Vec<String>>,
        active_wcs: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let active_wcs = active_wcs.unwrap_or_else(|| "G54".to_string());
        let inner = self.inner.clone();
        spawn_future(py, async move {
            inner.home(axes, &active_wcs).await.map_err(connection)?;
            Ok(Python::attach(|py| py.None()))
        })
    }

    fn move_to<'py>(
        &self,
        py: Python<'py>,
        speed: f64,
        x: f64,
        y: f64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            inner.move_to(speed, x, y).await.map_err(connection)?;
            Ok(Python::attach(|py| py.None()))
        })
    }

    fn jog<'py>(
        &self,
        py: Python<'py>,
        speed: f64,
        deltas: Py<PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let deltas = py_to_json(deltas.bind(py))?;
        let inner = self.inner.clone();
        spawn_future(py, async move {
            inner.jog(speed, deltas).await.map_err(connection)?;
            Ok(Python::attach(|py| py.None()))
        })
    }

    fn select_tool<'py>(
        &self,
        py: Python<'py>,
        tool_number: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            inner.select_tool(tool_number).await.map_err(connection)?;
            Ok(Python::attach(|py| py.None()))
        })
    }

    #[pyo3(signature = (power = None))]
    fn set_power<'py>(
        &self,
        py: Python<'py>,
        power: Option<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            inner.set_power(power).await.map_err(connection)?;
            Ok(Python::attach(|py| py.None()))
        })
    }

    #[pyo3(signature = (power = None))]
    fn set_focus_power<'py>(
        &self,
        py: Python<'py>,
        power: Option<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            inner.set_focus_power(power).await.map_err(connection)?;
            Ok(Python::attach(|py| py.None()))
        })
    }

    fn read_settings<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            let value = inner.read_settings().await.map_err(connection)?;
            Python::attach(|py| value_to_py(py, &value))
        })
    }

    fn write_setting<'py>(
        &self,
        py: Python<'py>,
        key: String,
        value: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            inner
                .write_setting(&key, &value)
                .await
                .map_err(connection)?;
            Ok(Python::attach(|py| py.None()))
        })
    }

    fn set_wcs_offset<'py>(
        &self,
        py: Python<'py>,
        wcs_slot: String,
        x: f64,
        y: f64,
        z: Option<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            inner
                .set_wcs_offset(&wcs_slot, x, y, z)
                .await
                .map_err(connection)?;
            Ok(Python::attach(|py| py.None()))
        })
    }

    fn read_wcs_offsets<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            let value = inner.read_wcs_offsets().await.map_err(connection)?;
            Python::attach(|py| value_to_py(py, &value))
        })
    }

    fn read_parser_state<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            let value = inner.read_parser_state().await.map_err(connection)?;
            Python::attach(|py| value_to_py(py, &value))
        })
    }

    fn run_probe_cycle<'py>(
        &self,
        py: Python<'py>,
        axis_letter: String,
        max_travel: f64,
        feed_rate: f64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            let value = inner
                .run_probe_cycle(&axis_letter, max_travel, feed_rate)
                .await
                .map_err(connection)?;
            Python::attach(|py| value_to_py(py, &value))
        })
    }

    fn detect_unit_system<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            let value = inner.detect_unit_system().await.map_err(connection)?;
            Python::attach(|py| value_to_py(py, &value))
        })
    }

    fn clear_alarm<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            inner.clear_alarm().await.map_err(connection)?;
            Ok(Python::attach(|py| py.None()))
        })
    }

    fn update_dialect(
        &self,
        py: Python<'_>,
        dialect: Py<PyAny>,
    ) -> PyResult<()> {
        let dialect = config_to_value(py, &dialect)?;
        self.inner.update_dialect(dialect);
        Ok(())
    }
}

fn connection(message: String) -> PyErr {
    PyConnectionError::new_err(message)
}

fn config_to_value(py: Python<'_>, value: &Py<PyAny>) -> PyResult<Value> {
    let bound = value.bind(py);
    if let Ok(dict) = bound.cast::<PyDict>() {
        return py_to_json(dict.as_any());
    }
    if bound.hasattr("to_dict")? {
        let converted = bound.call_method0("to_dict")?;
        return py_to_json(&converted);
    }
    py_to_json(bound)
}

fn op_map_to_map(
    py: Python<'_>,
    value: &Py<PyAny>,
) -> PyResult<HashMap<usize, i64>> {
    let mut result = HashMap::new();
    let bound = value.bind(py);
    if let Ok(dict) = bound.cast::<PyDict>() {
        for (key, value) in dict.iter() {
            result.insert(key.extract()?, value.extract()?);
        }
        return Ok(result);
    }
    for pair in bound.try_iter()? {
        let pair = pair?;
        let pair = pair.cast::<PyTuple>()?;
        result
            .insert(pair.get_item(0)?.extract()?, pair.get_item(1)?.extract()?);
    }
    Ok(result)
}

fn parse_endpoint(
    py: Python<'_>,
    endpoint: &Py<PyAny>,
) -> PyResult<RemoteEndpoint> {
    let bound = endpoint.bind(py);
    let value = if let Ok(url) = bound.extract::<String>() {
        parse_url(&url)?
    } else if bound.hasattr("to_dict")? {
        let dict = bound.call_method0("to_dict")?;
        py_to_json(&dict)?
    } else {
        py_to_json(bound)?
    };
    let host = value
        .get("remote_host")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            PyValueError::new_err(
                "endpoint needs remote_host or a raydriver:// url",
            )
        })?
        .to_string();
    Ok(RemoteEndpoint {
        host,
        port: value
            .get("remote_port")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_PORT as u64) as u16,
        token: value
            .get("token")
            .and_then(Value::as_str)
            .map(str::to_string),
        tls_ca: value
            .get("tls_ca")
            .and_then(Value::as_str)
            .map(str::to_string),
        tls_hostname: value
            .get("tls_hostname")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn parse_url(url: &str) -> PyResult<Value> {
    let Some(rest) = url.strip_prefix("raydriver://") else {
        return Err(PyValueError::new_err(format!(
            "unsupported remote url {url:?}"
        )));
    };
    let (authority, token) = match rest.split_once('#') {
        Some((authority, token)) => (authority, Some(token)),
        None => (rest, None),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (
            host,
            port.parse::<u16>().map_err(|_| {
                PyValueError::new_err(format!("invalid remote port {port:?}"))
            })?,
        ),
        None => (authority, DEFAULT_PORT),
    };
    if host.is_empty() {
        return Err(PyValueError::new_err(format!(
            "remote url has no host: {url:?}"
        )));
    }
    Ok(json!({
        "remote_host": host,
        "remote_port": port,
        "token": token.filter(|token| !token.is_empty()),
    }))
}

fn remote_endpoint_from_config(
    config: &Value,
) -> PyResult<Option<RemoteEndpoint>> {
    if let Some(url) = config.get("url").and_then(Value::as_str) {
        let endpoint = parse_url(url)?;
        return Ok(Some(RemoteEndpoint {
            host: endpoint
                .get("remote_host")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            port: endpoint
                .get("remote_port")
                .and_then(Value::as_u64)
                .unwrap_or(DEFAULT_PORT as u64) as u16,
            token: endpoint
                .get("token")
                .and_then(Value::as_str)
                .map(str::to_string),
            tls_ca: config
                .get("tls_ca")
                .and_then(Value::as_str)
                .map(str::to_string),
            tls_hostname: config
                .get("tls_hostname")
                .and_then(Value::as_str)
                .map(str::to_string),
        }));
    }
    let Some(host) = config.get("remote_host").and_then(Value::as_str) else {
        return Ok(None);
    };
    Ok(Some(RemoteEndpoint {
        host: host.to_string(),
        port: config
            .get("remote_port")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_PORT as u64) as u16,
        token: config
            .get("token")
            .and_then(Value::as_str)
            .map(str::to_string),
        tls_ca: config
            .get("tls_ca")
            .and_then(Value::as_str)
            .map(str::to_string),
        tls_hostname: config
            .get("tls_hostname")
            .and_then(Value::as_str)
            .map(str::to_string),
    }))
}

const REMOTE_KEYS: [&str; 6] = [
    "remote_host",
    "remote_port",
    "token",
    "tls_ca",
    "tls_hostname",
    "url",
];

#[gen_stub_pyfunction(module = "raydriver.remote.client")]
#[pyfunction]
pub(crate) fn create_session(
    py: Python<'_>,
    protocol_name: String,
    config: Py<PyAny>,
    dialect: Option<Py<PyAny>>,
    event_callback: Option<Py<PyAny>>,
) -> PyResult<Py<PyAny>> {
    let config_value = config_to_value(py, &config)?;
    let dialect_value = dialect
        .as_ref()
        .map(|dialect| config_to_value(py, dialect))
        .transpose()?;
    let Some(endpoint) = remote_endpoint_from_config(&config_value)? else {
        let config = value_to_py(py, &config_value)?;
        let dialect = match dialect_value {
            Some(dialect) => value_to_py(py, &dialect)?,
            None => py.None(),
        };
        return PySessionFactory::default_session(
            py,
            &protocol_name,
            config,
            dialect,
            event_callback.unwrap_or_else(|| py.None()),
        );
    };
    let local_config = match config_value {
        Value::Object(mut map) => {
            for key in REMOTE_KEYS {
                map.remove(key);
            }
            Value::Object(map)
        }
        other => other,
    };
    let loop_ = running_loop(py).ok();
    let events = Arc::new(PyRemoteEvents::new(event_callback, loop_));
    let inner = CoreRemoteSession::new(
        Some(protocol_name),
        local_config,
        dialect_value,
        endpoint,
        None,
        None,
        events.clone(),
    );
    Ok(Py::new(py, RemoteSession { inner, events })?.into_any())
}

#[gen_stub_pyfunction(module = "raydriver.remote.client")]
#[pyfunction]
#[pyo3(signature = (
    endpoint,
    device = None,
    session_id = None,
    config = None,
    dialect = None,
    event_callback = None,
))]
pub(crate) fn open_session(
    py: Python<'_>,
    endpoint: Py<PyAny>,
    device: Option<String>,
    session_id: Option<String>,
    config: Option<Py<PyAny>>,
    dialect: Option<Py<PyAny>>,
    event_callback: Option<Py<PyAny>>,
) -> PyResult<RemoteSession> {
    if device.is_none() && session_id.is_none() {
        return Err(PyValueError::new_err(
            "open_session needs a device or session_id",
        ));
    }
    let endpoint = parse_endpoint(py, &endpoint)?;
    let config = match config {
        Some(config) => config_to_value(py, &config)?,
        None => Value::Object(Default::default()),
    };
    let dialect = dialect
        .map(|dialect| config_to_value(py, &dialect))
        .transpose()?;
    let loop_ = running_loop(py).ok();
    let events = Arc::new(PyRemoteEvents::new(event_callback, loop_));
    let inner = CoreRemoteSession::new(
        None,
        config,
        dialect,
        endpoint,
        device,
        session_id,
        events.clone(),
    );
    Ok(RemoteSession { inner, events })
}

#[gen_stub_pyfunction(module = "raydriver.remote.client")]
#[pyfunction]
pub(crate) fn list_devices<'py>(
    py: Python<'py>,
    endpoint: Py<PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let endpoint = parse_endpoint(py, &endpoint)?;
    spawn_future(py, async move {
        let devices =
            core::list_devices(&endpoint).await.map_err(connection)?;
        Python::attach(|py| value_to_py(py, &Value::Array(devices)))
    })
}

#[gen_stub_pyfunction(module = "raydriver.remote.client")]
#[pyfunction]
pub(crate) fn list_protocols<'py>(
    py: Python<'py>,
    endpoint: Py<PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let endpoint = parse_endpoint(py, &endpoint)?;
    spawn_future(py, async move {
        let protocols =
            core::list_protocols(&endpoint).await.map_err(connection)?;
        Python::attach(|py| value_to_py(py, &Value::Array(protocols)))
    })
}

pyo3_stub_gen::module_variable!("raydriver.remote.client", "DEFAULT_PORT", u16);
pyo3_stub_gen::module_variable!(
    "raydriver.remote.client",
    "REMOTE_KEYS",
    Vec<&'static str>
);
