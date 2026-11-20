//! Python bindings for the remote daemon.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use pyo3::exceptions::PyOverflowError;
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use serde_json::Value;

use crate::core::remote::daemon::{
    self as core, ClientQueue as CoreClientQueue, Daemon as CoreDaemon,
    DaemonConfig as CoreConfig, DaemonError, SEND_QUEUE_LIMIT,
};
use crate::python::core::json::value_to_py;
use crate::python::pyfuture::spawn_future;
use crate::python::remote::bridge::{py_to_json, running_loop};
use crate::python::remote::session::PySessionFactory;

#[gen_stub_pyclass]
#[pyclass(module = "raydriver.remote")]
pub struct DaemonConfig {
    pub(crate) inner: Arc<Mutex<CoreConfig>>,
}

#[gen_stub_pymethods]
#[pymethods]
impl DaemonConfig {
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        host = "127.0.0.1",
        port = core::DEFAULT_PORT,
        token = None,
        tls_cert = None,
        tls_key = None,
        keepalive_interval = 2.0,
        keepalive_misses = 3,
        lease_grace = 5.0,
        ring_size = 1000,
        cameras = None,
        sessions = None,
        mdns = true,
    ))]
    fn new(
        py: Python<'_>,
        host: &str,
        port: u16,
        token: Option<String>,
        tls_cert: Option<String>,
        tls_key: Option<String>,
        keepalive_interval: f64,
        keepalive_misses: u32,
        lease_grace: f64,
        ring_size: usize,
        cameras: Option<Py<PyAny>>,
        sessions: Option<Py<PyAny>>,
        mdns: bool,
    ) -> PyResult<Self> {
        let cameras = match cameras {
            Some(cameras) => py_to_json(cameras.bind(py))?,
            None => Value::Array(Vec::new()),
        };
        let sessions = match sessions {
            Some(sessions) => py_to_json(sessions.bind(py))?,
            None => Value::Array(Vec::new()),
        };
        Ok(Self {
            inner: Arc::new(Mutex::new(CoreConfig {
                host: host.to_string(),
                port,
                token,
                tls_cert,
                tls_key,
                keepalive_interval,
                keepalive_misses,
                lease_grace,
                ring_size,
                cameras,
                sessions,
                mdns,
            })),
        })
    }

    #[getter]
    fn host(&self) -> String {
        self.inner.lock().unwrap().host.clone()
    }

    #[setter]
    fn set_host(&self, value: String) {
        self.inner.lock().unwrap().host = value;
    }

    #[getter]
    fn port(&self) -> u16 {
        self.inner.lock().unwrap().port
    }

    #[setter]
    fn set_port(&self, value: u16) {
        self.inner.lock().unwrap().port = value;
    }

    #[getter]
    fn token(&self) -> Option<String> {
        self.inner.lock().unwrap().token.clone()
    }

    #[setter]
    fn set_token(&self, value: Option<String>) {
        self.inner.lock().unwrap().token = value;
    }

    #[getter]
    fn tls_cert(&self) -> Option<String> {
        self.inner.lock().unwrap().tls_cert.clone()
    }

    #[setter]
    fn set_tls_cert(&self, value: Option<String>) {
        self.inner.lock().unwrap().tls_cert = value;
    }

    #[getter]
    fn tls_key(&self) -> Option<String> {
        self.inner.lock().unwrap().tls_key.clone()
    }

    #[setter]
    fn set_tls_key(&self, value: Option<String>) {
        self.inner.lock().unwrap().tls_key = value;
    }

    #[getter]
    fn keepalive_interval(&self) -> f64 {
        self.inner.lock().unwrap().keepalive_interval
    }

    #[setter]
    fn set_keepalive_interval(&self, value: f64) {
        self.inner.lock().unwrap().keepalive_interval = value;
    }

    #[getter]
    fn keepalive_misses(&self) -> u32 {
        self.inner.lock().unwrap().keepalive_misses
    }

    #[setter]
    fn set_keepalive_misses(&self, value: u32) {
        self.inner.lock().unwrap().keepalive_misses = value;
    }

    #[getter]
    fn lease_grace(&self) -> f64 {
        self.inner.lock().unwrap().lease_grace
    }

    #[setter]
    fn set_lease_grace(&self, value: f64) {
        self.inner.lock().unwrap().lease_grace = value;
    }

    #[getter]
    fn ring_size(&self) -> usize {
        self.inner.lock().unwrap().ring_size
    }

    #[setter]
    fn set_ring_size(&self, value: usize) {
        self.inner.lock().unwrap().ring_size = value;
    }

    #[getter]
    fn cameras(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let value = self.inner.lock().unwrap().cameras.clone();
        value_to_py(py, &value)
    }

    #[setter]
    fn set_cameras(&self, value: Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.lock().unwrap().cameras = py_to_json(&value)?;
        Ok(())
    }

    #[getter]
    fn sessions(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let value = self.inner.lock().unwrap().sessions.clone();
        value_to_py(py, &value)
    }

    #[setter]
    fn set_sessions(&self, value: Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.lock().unwrap().sessions = py_to_json(&value)?;
        Ok(())
    }

    #[getter]
    fn mdns(&self) -> bool {
        self.inner.lock().unwrap().mdns
    }

    #[setter]
    fn set_mdns(&self, value: bool) {
        self.inner.lock().unwrap().mdns = value;
    }
}

#[gen_stub_pyclass]
#[pyclass(module = "raydriver.remote")]
pub struct Daemon {
    inner: Arc<CoreDaemon>,
    config: Arc<Mutex<CoreConfig>>,
}

#[gen_stub_pymethods]
#[pymethods]
impl Daemon {
    #[new]
    #[pyo3(signature = (config = None, session_factory = None))]
    fn new(
        py: Python<'_>,
        config: Option<PyRef<'_, DaemonConfig>>,
        session_factory: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let config = config
            .map(|config| config.inner.clone())
            .unwrap_or_else(|| Arc::new(Mutex::new(CoreConfig::default())));
        let loop_ = running_loop(py)?;
        let factory = Arc::new(PySessionFactory::new(session_factory, loop_));
        let inner = Arc::new(CoreDaemon::new(config.clone(), factory));
        Ok(Self { inner, config })
    }

    #[getter]
    fn config(&self) -> DaemonConfig {
        DaemonConfig {
            inner: self.config.clone(),
        }
    }

    fn start<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            let started = inner.start().await.map_err(core_error)?;
            Python::attach(|py| {
                Ok(Py::new(py, StartedServer { addr: started.addr })?
                    .into_any())
            })
        })
    }

    fn stop<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            inner.stop().await;
            Ok(Python::attach(|py| py.None()))
        })
    }
}

fn core_error(err: DaemonError) -> PyErr {
    match err {
        DaemonError::Insecure(message) => Python::attach(|py| {
            let result: PyResult<PyErr> = (|| {
                let module = py.import("raydriver.analyzer.listener")?;
                let cls = module.getattr("InsecureListenerError")?;
                let value = cls.call1((message,))?;
                Ok(PyErr::from_value(value))
            })();
            result.unwrap_or_else(|_| {
                pyo3::exceptions::PyValueError::new_err("insecure listener")
            })
        }),
        other => pyo3::exceptions::PyRuntimeError::new_err(other.to_string()),
    }
}

#[pyclass(module = "raydriver.remote")]
pub struct SocketInfo {
    addr: SocketAddr,
}

#[pymethods]
impl SocketInfo {
    fn getsockname(&self) -> (String, u16) {
        (self.addr.ip().to_string(), self.addr.port())
    }
}

#[pyclass(module = "raydriver.remote")]
pub struct StartedServer {
    addr: SocketAddr,
}

#[pymethods]
impl StartedServer {
    #[getter]
    fn sockets(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let socket = Py::new(py, SocketInfo { addr: self.addr })?;
        Ok(PyTuple::new(py, [socket])?.unbind().into_any())
    }
}

#[gen_stub_pyclass]
#[pyclass(module = "raydriver.remote")]
pub struct ClientQueue {
    inner: Arc<CoreClientQueue>,
}

#[gen_stub_pymethods]
#[pymethods]
impl ClientQueue {
    #[new]
    #[pyo3(signature = (limit = SEND_QUEUE_LIMIT))]
    fn new(limit: usize) -> Self {
        Self {
            inner: Arc::new(CoreClientQueue::new(limit)),
        }
    }

    fn put(&self, py: Python<'_>, frame: Py<PyAny>) -> PyResult<()> {
        let frame = py_to_json(frame.bind(py))?;
        if !self.inner.push(frame) {
            return Err(PyOverflowError::new_err("client send queue overflow"));
        }
        Ok(())
    }

    fn get<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        spawn_future(py, async move {
            let frame = inner.get().await;
            Python::attach(|py| value_to_py(py, &frame))
        })
    }
}
