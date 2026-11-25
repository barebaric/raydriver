//! `GrblNetworkDriver`: GRBL behind the ESP3D / FluidNC HTTP API.

use std::sync::Arc;

use crate::core::dialect::Dialect;
use crate::core::driver::{delegate_driver, DriverEvents};
use crate::core::transport::TransportKind;
use crate::esp3d::transport::{Esp3dConfig, Esp3dTransport};

use super::stream::{GrblStream, GrblStreamConfig};

/// A GRBL-compatible device on the FluidNC / ESP3D HTTP + WebSocket
/// interface.  Commands travel over HTTP; jobs are uploaded whole and
/// started, and status is reported over the WebSocket.
pub struct GrblNetworkDriver {
    stream: Arc<GrblStream>,
}

impl GrblNetworkDriver {
    /// Build a driver from the ESP3D connection parameters.
    pub fn new(
        esp3d: Esp3dConfig,
        config: GrblStreamConfig,
        dialect: Dialect,
        events: Arc<dyn DriverEvents>,
    ) -> Arc<Self> {
        Self::with_transport(
            TransportKind::Custom(Esp3dTransport::new(esp3d)),
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

delegate_driver!(GrblNetworkDriver, stream);
