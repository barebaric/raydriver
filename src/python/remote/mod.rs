//! The `raydriver.remote` submodule: daemon, client and codec.

pub(crate) mod bridge;
pub(crate) mod client;
pub(crate) mod daemon;
pub(crate) mod events;
pub(crate) mod inspect;
pub(crate) mod mdns;
pub(crate) mod protocol;
pub(crate) mod session;

use pyo3::prelude::*;
use pyo3::types::PySet;

use crate::core::remote::registry;

pub(crate) const MODULE_DOC: &str = "\
Remote sessions: a device-host daemon, a client proxy and the \
versioned wire codec that connects them.
";

pyo3_stub_gen::module_doc!("raydriver.remote", "{}", MODULE_DOC);

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let remote = PyModule::new(py, "remote")?;
    remote.setattr("__doc__", MODULE_DOC)?;
    remote.add(
        "__all__",
        vec![
            "Announcer",
            "ClientQueue",
            "Daemon",
            "DaemonConfig",
            "RemoteSession",
            "ServiceInfo",
            "announce",
            "browse_daemons",
            "create_session",
            "daemon_rpc",
            "decode_event",
            "decode_frame",
            "decode_state",
            "encode_event",
            "encode_frame",
            "encode_state",
            "error_response",
            "event_frame",
            "list_devices",
            "list_protocols",
            "open_session",
            "request",
            "response",
            "snapshot",
            "welcome_payload",
            "welcome_probe",
        ],
    )?;
    add_protocol_constants(&remote)?;
    remote.add_function(wrap_pyfunction!(protocol::encode_frame, &remote)?)?;
    remote.add_function(wrap_pyfunction!(protocol::decode_frame, &remote)?)?;
    remote.add_function(wrap_pyfunction!(protocol::request, &remote)?)?;
    remote.add_function(wrap_pyfunction!(protocol::response, &remote)?)?;
    remote
        .add_function(wrap_pyfunction!(protocol::error_response, &remote)?)?;
    remote.add_function(wrap_pyfunction!(protocol::event_frame, &remote)?)?;
    remote
        .add_function(wrap_pyfunction!(protocol::welcome_payload, &remote)?)?;
    remote.add_function(wrap_pyfunction!(protocol::encode_state, &remote)?)?;
    remote.add_function(wrap_pyfunction!(protocol::decode_state, &remote)?)?;
    remote.add_function(wrap_pyfunction!(protocol::encode_event, &remote)?)?;
    remote.add_function(wrap_pyfunction!(protocol::decode_event, &remote)?)?;

    remote.add(
        "REMOTE_KEYS",
        (
            "remote_host",
            "remote_port",
            "token",
            "tls_ca",
            "tls_hostname",
            "url",
        ),
    )?;
    remote.add("DEFAULT_PORT", crate::core::remote::daemon::DEFAULT_PORT)?;
    remote.add(
        "DEFAULT_RING_SIZE",
        crate::core::remote::daemon::DEFAULT_RING_SIZE,
    )?;
    remote.add(
        "DEFAULT_LEASE_GRACE",
        crate::core::remote::daemon::DEFAULT_LEASE_GRACE,
    )?;
    remote.add(
        "SEND_QUEUE_LIMIT",
        crate::core::remote::daemon::SEND_QUEUE_LIMIT,
    )?;
    remote.add_class::<daemon::DaemonConfig>()?;
    remote.add_class::<daemon::Daemon>()?;
    remote.add_class::<daemon::ClientQueue>()?;

    remote.add_class::<client::RemoteSession>()?;
    remote.add_function(wrap_pyfunction!(client::create_session, &remote)?)?;
    remote.add_function(wrap_pyfunction!(client::open_session, &remote)?)?;
    remote.add_function(wrap_pyfunction!(client::list_devices, &remote)?)?;
    remote.add_function(wrap_pyfunction!(client::list_protocols, &remote)?)?;

    remote.add("SERVICE_TYPE", crate::core::remote::mdns::SERVICE_TYPE)?;
    remote.add_class::<mdns::Announcer>()?;
    remote.add_class::<mdns::ServiceInfo>()?;
    remote.add_function(wrap_pyfunction!(mdns::announce, &remote)?)?;
    remote.add_function(wrap_pyfunction!(mdns::browse_daemons, &remote)?)?;
    remote.add_function(wrap_pyfunction!(mdns::_announce_address, &remote)?)?;
    remote.add_function(wrap_pyfunction!(mdns::_service_name, &remote)?)?;
    remote.add_function(wrap_pyfunction!(mdns::_service_name_for, &remote)?)?;

    remote.add_function(wrap_pyfunction!(inspect::daemon_rpc, &remote)?)?;
    remote.add_function(wrap_pyfunction!(inspect::welcome_probe, &remote)?)?;
    remote.add_function(wrap_pyfunction!(inspect::snapshot, &remote)?)?;

    m.add_submodule(&remote)?;
    Ok(())
}

fn add_protocol_constants(protocol: &Bound<'_, PyModule>) -> PyResult<()> {
    use crate::core::remote::protocol as core;
    protocol.add("PROTOCOL_VERSION", core::PROTOCOL_VERSION)?;
    protocol.add("MAX_FRAME_BYTES", core::MAX_FRAME_BYTES)?;
    protocol.add(
        "DEFAULT_KEEPALIVE_INTERVAL",
        core::DEFAULT_KEEPALIVE_INTERVAL,
    )?;
    protocol.add("DEFAULT_KEEPALIVE_MISSES", core::DEFAULT_KEEPALIVE_MISSES)?;
    for (name, value) in [
        ("AUTH", core::AUTH),
        ("PING", core::PING),
        ("PONG", core::PONG),
        ("DEVICE_LIST", core::DEVICE_LIST),
        ("PROTOCOL_LIST", core::PROTOCOL_LIST),
        ("SESSION_LIST", core::SESSION_LIST),
        ("SESSION_OPEN", core::SESSION_OPEN),
        ("SESSION_ATTACH", core::SESSION_ATTACH),
        ("SESSION_DETACH", core::SESSION_DETACH),
        ("SESSION_CLOSE", core::SESSION_CLOSE),
        ("SESSION_COMMAND", core::SESSION_COMMAND),
        ("SESSION_PROPERTY", core::SESSION_PROPERTY),
        ("CLIENT_LIST", core::CLIENT_LIST),
        ("WELCOME", core::WELCOME),
        ("EVENT", core::EVENT),
        ("BAD_REQUEST", core::BAD_REQUEST),
        ("BAD_VERSION", core::BAD_VERSION),
        ("AUTH_FAILED", core::AUTH_FAILED),
        ("UNKNOWN_TYPE", core::UNKNOWN_TYPE),
        ("UNKNOWN_SESSION", core::UNKNOWN_SESSION),
        ("UNKNOWN_DEVICE", core::UNKNOWN_DEVICE),
        ("UNKNOWN_METHOD", core::UNKNOWN_METHOD),
        ("UNKNOWN_PROPERTY", core::UNKNOWN_PROPERTY),
        ("LEASE_TAKEN", core::LEASE_TAKEN),
        ("LEASE_HELD", core::LEASE_HELD),
        ("JOB_RUNNING", core::JOB_RUNNING),
        ("FRAME_TOO_LARGE", core::FRAME_TOO_LARGE),
        ("INTERNAL", core::INTERNAL),
        ("KIND_PROGRESS", core::KIND_PROGRESS),
        ("KIND_JOB_ERROR", core::KIND_JOB_ERROR),
        ("KIND_LEASE", core::KIND_LEASE),
    ] {
        protocol.add(name, value)?;
    }
    let request_types = PySet::empty(protocol.py())?;
    for name in [
        core::AUTH,
        core::PING,
        core::PONG,
        core::DEVICE_LIST,
        core::PROTOCOL_LIST,
        core::SESSION_LIST,
        core::SESSION_OPEN,
        core::SESSION_ATTACH,
        core::SESSION_DETACH,
        core::SESSION_CLOSE,
        core::SESSION_COMMAND,
        core::SESSION_PROPERTY,
        core::CLIENT_LIST,
    ] {
        request_types.add(name)?;
    }
    protocol.add("REQUEST_TYPES", request_types)?;
    let names = registry::names();
    protocol.add("PROTOCOLS", names.to_vec())?;
    let mut methods: Vec<&str> = Vec::new();
    let mut properties: Vec<&str> = Vec::new();
    for name in names {
        if let Some(info) = registry::protocol_info(name) {
            methods.extend(info.methods);
            properties.extend(info.properties);
        }
    }
    methods.sort_unstable();
    methods.dedup();
    properties.sort_unstable();
    properties.dedup();
    protocol.add("SESSION_METHODS", methods)?;
    protocol.add("SESSION_PROPERTIES", properties)?;
    Ok(())
}
