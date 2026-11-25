//! `GrblTelnetDriver`: GRBL over a telnet connection.

use std::sync::Arc;

use crate::core::dialect::Dialect;
use crate::core::driver::{
    delegate_driver,
    info::{caps, DEFAULT_SUPPORTED_WCS},
    ConnectionKind, DriverEvents, DriverInfo, DriverMaturity,
};
use crate::core::transport::TransportKind;

use super::stream::{GrblStream, GrblStreamConfig};

/// Typed identity and capabilities of the telnet GRBL driver.
pub static INFO: DriverInfo = DriverInfo {
    name: "GrblTelnetDriver",
    connection: ConnectionKind::Telnet,
    maturity: DriverMaturity::Stable,
    caps: caps(
        true, true, true, true, false, true, true, true, false, true, true,
    ),
    uses_gcode: true,
    machine_space_wcs: "G53",
    supported_wcs: DEFAULT_SUPPORTED_WCS,
    config_keys: &[
        "host",
        "tcp_port",
        "poll_status_while_running",
        "deadlock_detection",
        "homing_timeout",
    ],
    fingerprint: None,
};

/// A GRBL-compatible device reachable over a telnet stream.
pub struct GrblTelnetDriver {
    stream: Arc<GrblStream>,
}

impl GrblTelnetDriver {
    /// Build a driver for *host*:*port*.
    pub fn new(
        host: impl Into<String>,
        port: u16,
        config: GrblStreamConfig,
        dialect: Dialect,
        events: Arc<dyn DriverEvents>,
    ) -> Arc<Self> {
        Self::with_transport(
            TransportKind::Telnet {
                host: host.into(),
                port,
            },
            config,
            dialect,
            events,
        )
    }

    /// Build a driver around an arbitrary transport (tests, wasm).
    pub fn with_transport(
        transport: TransportKind,
        config: GrblStreamConfig,
        dialect: Dialect,
        events: Arc<dyn DriverEvents>,
    ) -> Arc<Self> {
        Arc::new(Self {
            stream: GrblStream::new(transport, config, &INFO, dialect, events),
        })
    }
}

delegate_driver!(GrblTelnetDriver, stream);
