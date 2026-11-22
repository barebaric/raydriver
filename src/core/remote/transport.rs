//! Control-protocol streams: TCP or TLS, with frame helpers.
//!
//! Shared by the daemon client and the discovery probes, so there is
//! one framing implementation and one TLS setup.

use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::RootCertStore;
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::core::remote::protocol;

/// A control-protocol stream, plain TCP or TLS.
pub trait ControlStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> ControlStream for T {}

pub type BoxedStream = Box<dyn ControlStream>;

pub async fn connect_stream(
    host: &str,
    port: u16,
    tls_ca: Option<&str>,
    tls_hostname: Option<&str>,
    timeout: Duration,
) -> Result<BoxedStream, String> {
    let tcp = tokio::time::timeout(timeout, TcpStream::connect((host, port)))
        .await
        .map_err(|_| format!("connecting to {host}:{port} timed out"))?
        .map_err(|err| format!("cannot connect to {host}:{port}: {err}"))?;
    let Some(ca_path) = tls_ca else {
        return Ok(Box::new(tcp));
    };

    let roots = load_roots(ca_path)?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|err| format!("TLS configuration failed: {err}"))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let name = tls_hostname.unwrap_or(host);
    let server_name = ServerName::try_from(name.to_string())
        .map_err(|_| format!("invalid TLS server name {name:?}"))?;
    let stream =
        tokio::time::timeout(timeout, connector.connect(server_name, tcp))
            .await
            .map_err(|_| "TLS handshake timed out".to_string())?
            .map_err(|err| format!("TLS handshake failed: {err}"))?;
    Ok(Box::new(stream))
}

fn load_roots(path: &str) -> Result<RootCertStore, String> {
    let certificates = CertificateDer::pem_file_iter(path)
        .map_err(|err| format!("cannot read TLS CA {path}: {err}"))?;
    let mut roots = RootCertStore::empty();
    let mut added = 0;
    for certificate in certificates {
        let certificate = certificate
            .map_err(|err| format!("invalid TLS CA {path}: {err}"))?;
        roots
            .add(certificate)
            .map_err(|err| format!("invalid TLS CA {path}: {err}"))?;
        added += 1;
    }
    if added == 0 {
        return Err(format!("no certificates in TLS CA {path}"));
    }
    Ok(roots)
}

pub fn is_loopback(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .map(|address| address.is_loopback())
        .unwrap_or(false)
}

/// Read one frame, skipping keepalive pings.
pub async fn read_message<S: AsyncRead + Unpin>(
    stream: &mut S,
    timeout: Duration,
) -> Result<Value, String> {
    loop {
        let message = read_frame(stream, timeout).await?;
        if message.get("type").and_then(Value::as_str) == Some(protocol::PING) {
            continue;
        }
        return Ok(message);
    }
}

/// Read one frame within *timeout*.
pub async fn read_frame<S: AsyncRead + Unpin>(
    stream: &mut S,
    timeout: Duration,
) -> Result<Value, String> {
    tokio::time::timeout(timeout, protocol::read_frame(stream))
        .await
        .map_err(|_| "timed out reading frame".to_string())?
        .map_err(|err| err.to_string())
}

/// Write one frame.
pub async fn write_frame<S: AsyncWrite + Unpin>(
    stream: &mut S,
    message: &Value,
) -> Result<(), String> {
    protocol::write_frame(stream, message)
        .await
        .map_err(|err| err.to_string())
}
