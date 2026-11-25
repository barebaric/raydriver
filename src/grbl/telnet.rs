//! `GrblTelnetDriver`: GRBL over a telnet connection.

use std::sync::Arc;

use crate::core::dialect::Dialect;
use crate::core::driver::{delegate_driver, DriverEvents};
use crate::core::transport::TransportKind;

use super::stream::{GrblStream, GrblStreamConfig};

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
            stream: GrblStream::new(transport, config, dialect, events),
        })
    }
}

delegate_driver!(GrblTelnetDriver, stream);
