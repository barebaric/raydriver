//! Subnet scanning for networked controllers.
//!
//! Some controllers (e.g. grblHAL boards with the raw telnet
//! service) announce nothing via mDNS. To find them, this scanner
//! TCP-connects to a fixed port on every host of the local subnets
//! and reports the hosts that accept the connection. Whether a
//! reachable host actually speaks a driver's protocol is decided
//! afterwards by the embedding application's fingerprint probes.
//!
//! Scans are bounded in concurrency and per-host connect time, and
//! only ever run on explicit request — never as a background poll.

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::sync::Semaphore;

/// Largest subnet scanned per interface, as a CIDR prefix. Wider
/// interfaces (e.g. /16) are narrowed to the /24 containing the
/// interface address so a scan stays a few hundred connects.
const MAX_SCAN_PREFIX: u8 = 24;

/// The local non-loopback IPv4 interfaces, as (address, prefixlen).
pub fn local_subnets() -> Vec<(Ipv4Addr, u8)> {
    let mut out = Vec::new();
    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for interface in interfaces {
            if let if_addrs::IfAddr::V4(v4) = interface.addr {
                if !v4.is_loopback() && !v4.ip.is_link_local() {
                    out.push((v4.ip, v4.prefixlen));
                }
            }
        }
    }
    out
}

/// The host addresses to probe on the subnet containing *addr*.
/// Networks wider than /24 are narrowed to the /24 around *addr*;
/// the network and broadcast addresses are excluded.
pub fn subnet_hosts(addr: Ipv4Addr, prefixlen: u8) -> Vec<String> {
    let prefix = prefixlen.max(MAX_SCAN_PREFIX);
    let mask = u32::MAX.checked_shl(32 - u32::from(prefix)).unwrap_or(0);
    let base = u32::from(addr) & mask;
    let count = 1u64 << (32 - u32::from(prefix));
    if count <= 2 {
        return (0..count)
            .map(|i| Ipv4Addr::from(base + i as u32).to_string())
            .collect();
    }
    (1..count - 1)
        .map(|i| Ipv4Addr::from(base + i as u32).to_string())
        .collect()
}

/// TCP-connects to *port* on every host, at most *concurrency*
/// connects in flight and *connect_timeout* per host, and returns
/// the hosts that accepted a connection, in input order.
pub async fn scan_subnet(
    hosts: Vec<String>,
    port: u16,
    connect_timeout: Duration,
    concurrency: usize,
) -> Vec<String> {
    let semaphore = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut handles = Vec::new();
    for host in hosts {
        let semaphore = semaphore.clone();
        handles.push(tokio::spawn(async move {
            let _permit = semaphore.acquire().await.ok()?;
            let reachable = tokio::time::timeout(
                connect_timeout,
                TcpStream::connect((host.as_str(), port)),
            )
            .await
            .map(|result| result.is_ok())
            .unwrap_or(false);
            reachable.then_some(host)
        }));
    }
    let mut out = Vec::new();
    for handle in handles {
        if let Ok(Some(host)) = handle.await {
            out.push(host);
        }
    }
    out
}
