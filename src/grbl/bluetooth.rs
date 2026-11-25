//! `GrblBluetoothDriver`: GRBL over Bluetooth SPP.

use std::sync::Arc;

use crate::core::dialect::Dialect;
use crate::core::driver::{delegate_driver, DriverEvents};
use crate::core::transport::TransportKind;

use super::stream::{GrblStream, GrblStreamConfig};

/// A GRBL-compatible device on a Bluetooth serial channel.
pub struct GrblBluetoothDriver {
    stream: Arc<GrblStream>,
}

impl GrblBluetoothDriver {
    /// Build a driver for *address* on *channel*.
    pub fn new(
        address: impl Into<String>,
        channel: u8,
        config: GrblStreamConfig,
        dialect: Dialect,
        events: Arc<dyn DriverEvents>,
    ) -> Arc<Self> {
        Self::with_transport(
            TransportKind::Bluetooth {
                address: address.into(),
                channel,
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

delegate_driver!(GrblBluetoothDriver, stream);
