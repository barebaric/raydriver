//! Device listing from raydriver daemons.
//!
//! A daemon is an endpoint, not a firmware: the probe speaks the
//! control protocol's welcome and lists the daemon's devices. The
//! wire format is a 4-byte big-endian payload length followed by one
//! UTF-8 JSON object, so the probe needs no firmware knowledge.
//! Token-protected daemons cannot be enumerated without credentials;
//! they surface as a single daemon row that seeds the endpoint vars.

use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// One frame = 4-byte big-endian length + UTF-8 JSON payload.
const MAX_FRAME_BYTES: u32 = 16 * 1024 * 1024;

/// Why a daemon could not be enumerated, for the daemon-level row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DaemonNote {
    Unreachable,
    TokenRequired,
    NoDevices,
}

/// One entry of a daemon probe: a hosted device, or a daemon-level
/// row when the endpoint cannot be enumerated.
#[derive(Clone, Debug)]
pub enum DaemonEntry {
    Device {
        /// Device resource reference on the daemon.
        device: String,
        /// Firmware protocol the device speaks (info only).
        protocol: String,
        /// Display label announced by the daemon, when any.
        label: Option<String>,
    },
    Row {
        note: DaemonNote,
    },
}

/// Everything one daemon probe yielded.
#[derive(Clone, Debug)]
pub struct DaemonReport {
    pub host: String,
    pub port: u16,
    /// mDNS instance name of the daemon, for the identity banner.
    pub name: String,
    pub entries: Vec<DaemonEntry>,
}

/// Connects to a daemon, reads its welcome and lists its devices.
/// Never raises: an unreachable or protected endpoint yields a
/// daemon row (or nothing when the peer does not speak the protocol).
pub async fn query_daemon(
    host: &str,
    port: u16,
    name: &str,
    timeout: Duration,
) -> DaemonReport {
    let entries = match query_inner(host, port, timeout).await {
        Ok(entries) => entries,
        Err(_) => vec![DaemonEntry::Row {
            note: DaemonNote::Unreachable,
        }],
    };
    DaemonReport {
        host: host.to_string(),
        port,
        name: name.to_string(),
        entries,
    }
}

async fn query_inner(
    host: &str,
    port: u16,
    timeout: Duration,
) -> Result<Vec<DaemonEntry>, ()> {
    let mut stream =
        tokio::time::timeout(timeout, TcpStream::connect((host, port)))
            .await
            .map_err(|_| ())?
            .map_err(|_| ())?;

    let welcome = read_message(&mut stream, timeout).await?;
    if welcome.get("type").and_then(Value::as_str) != Some("welcome") {
        return Ok(Vec::new());
    }
    if welcome
        .get("payload")
        .and_then(|p| p.get("auth_required"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(vec![DaemonEntry::Row {
            note: DaemonNote::TokenRequired,
        }]);
    }

    let request = json!({"id": 1, "type": "device/list", "payload": {}});
    write_frame(&mut stream, &request).await?;
    let reply = read_message(&mut stream, timeout).await?;
    if !reply.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(vec![DaemonEntry::Row {
            note: DaemonNote::TokenRequired,
        }]);
    }

    let mut entries = Vec::new();
    if let Some(devices) = reply
        .get("payload")
        .and_then(|p| p.get("devices"))
        .and_then(Value::as_array)
    {
        for device in devices {
            let Some(reference) = device.get("device").and_then(Value::as_str)
            else {
                continue;
            };
            if reference.is_empty() {
                continue;
            }
            entries.push(DaemonEntry::Device {
                device: reference.to_string(),
                protocol: device
                    .get("protocol")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                label: device
                    .get("label")
                    .and_then(Value::as_str)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string),
            });
        }
    }
    if entries.is_empty() {
        entries.push(DaemonEntry::Row {
            note: DaemonNote::NoDevices,
        });
    }
    Ok(entries)
}

/// Reads one frame, skipping keepalive pings.
async fn read_message(
    stream: &mut TcpStream,
    timeout: Duration,
) -> Result<Value, ()> {
    loop {
        let msg = read_frame(stream, timeout).await?;
        if msg.get("type").and_then(Value::as_str) == Some("ping") {
            continue;
        }
        return Ok(msg);
    }
}

async fn read_frame(
    stream: &mut TcpStream,
    timeout: Duration,
) -> Result<Value, ()> {
    let mut prefix = [0u8; 4];
    tokio::time::timeout(timeout, stream.read_exact(&mut prefix))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?;
    let size = u32::from_be_bytes(prefix);
    if size > MAX_FRAME_BYTES {
        return Err(());
    }
    let mut payload = vec![0u8; size as usize];
    tokio::time::timeout(timeout, stream.read_exact(&mut payload))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?;
    serde_json::from_slice(&payload).map_err(|_| ())
}

async fn write_frame(
    stream: &mut TcpStream,
    message: &Value,
) -> Result<(), ()> {
    let payload = serde_json::to_vec(message).map_err(|_| ())?;
    let mut frame = (payload.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(&payload);
    stream.write_all(&frame).await.map_err(|_| ())?;
    stream.flush().await.map_err(|_| ())
}
