//! Ruida transport implementations and factory.
//!
//! Counterpart of ``ruida_pa.ruidadriver.transport``.

pub mod base;
#[cfg(feature = "net")]
pub mod tcp_transport;
#[cfg(feature = "net")]
pub mod udp_transport;
#[cfg(feature = "serial")]
pub mod usb_transport;

pub use base::{TappedTransport, Transport, TransportKind};

use crate::core::driver::DriverError;

#[cfg(feature = "serial")]
use self::usb_transport::UsbTransport;
#[cfg(feature = "net")]
use self::{tcp_transport::TcpTransport, udp_transport::UdpTransport};

#[cfg(feature = "serial")]
pub(super) fn resolve_serial_device(device: &str) -> String {
    if device.contains(':') {
        let wanted = device.to_ascii_lowercase();
        if let Ok(ports) = serialport::available_ports() {
            for port in ports {
                if let serialport::SerialPortType::UsbPort(info) =
                    port.port_type
                {
                    let vid_pid = format!("{:04x}:{:04x}", info.vid, info.pid);
                    if vid_pid == wanted || port.port_name == device {
                        return port.port_name;
                    }
                }
            }
        }
    }
    if cfg!(unix) && !device.contains('/') {
        return format!("/dev/{device}");
    }
    device.to_string()
}

/// Opens a link for the given endpoint.
pub async fn open_transport(
    kind: TransportKind,
    host: Option<&str>,
    device: Option<&str>,
    port: u16,
) -> Result<Box<dyn Transport>, DriverError> {
    let _ = (host, device, port);
    match kind {
        #[cfg(feature = "serial")]
        TransportKind::Usb => {
            let device = device.ok_or_else(|| {
                DriverError::Device("no USB device configured".to_string())
            })?;
            Ok(Box::new(UsbTransport::open(device)?))
        }
        #[cfg(not(feature = "serial"))]
        TransportKind::Usb => Err(DriverError::Unsupported(
            "serial support is not compiled in".to_string(),
        )),
        #[cfg(feature = "net")]
        TransportKind::Udp => {
            let host = host.ok_or_else(|| {
                DriverError::Device("no controller host configured".to_string())
            })?;
            Ok(Box::new(UdpTransport::connect(host, port).await?))
        }
        #[cfg(not(feature = "net"))]
        TransportKind::Udp => Err(DriverError::Unsupported(
            "network support is not compiled in".to_string(),
        )),
        #[cfg(feature = "net")]
        TransportKind::Tcp => {
            let host = host.ok_or_else(|| {
                DriverError::Device("no controller host configured".to_string())
            })?;
            Ok(Box::new(TcpTransport::connect(host, port).await?))
        }
        #[cfg(not(feature = "net"))]
        TransportKind::Tcp => Err(DriverError::Unsupported(
            "network support is not compiled in".to_string(),
        )),
    }
}
