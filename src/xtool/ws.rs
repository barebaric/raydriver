//! Minimal RFC 6455 WebSocket client for the xTool notification
//! channel.
//!
//! Hand-rolled over a tokio `TcpStream` (no TLS: xTool machines
//! serve plain-text WebSocket on the LAN).  Only what the protocol
//! needs is implemented: the opening handshake, unbounded server
//! text messages, client-to-server frames sent masked, and
//! ping/pong/close handling.  Outgoing data frames are never sent —
//! the xTool WebSocket is a one-way notification channel.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use crate::core::session::SessionError;

const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// Handshake failed or the connection broke mid-frame.
pub struct WsError(pub String);

impl From<std::io::Error> for WsError {
    fn from(err: std::io::Error) -> Self {
        Self(err.to_string())
    }
}

impl From<WsError> for SessionError {
    fn from(err: WsError) -> Self {
        SessionError::Connection(err.0)
    }
}

/// A connected WebSocket.  Receive-only from the session's point of
/// view; pongs are answered transparently while waiting for the
/// next message.
pub struct WsClient {
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: tokio::net::tcp::OwnedWriteHalf,
}

impl WsClient {
    /// Open the TCP connection, perform the RFC 6455 handshake and
    /// return the established client.
    pub async fn connect(
        host: &str,
        port: u16,
        origin: Option<&str>,
        timeout: Duration,
    ) -> Result<Self, SessionError> {
        tokio::time::timeout(timeout, async {
            let stream = TcpStream::connect((host, port))
                .await
                .map_err(|e| WsError(e.to_string()))?;
            handshake(stream, host, port, origin)
                .await
                .map_err(|e| SessionError::Connection(e.0))
        })
        .await
        .map_err(|_| SessionError::Timeout)?
    }

    /// Receive the next text message, transparently answering pings
    /// and swallowing pongs.  Server-initiated closes surface as a
    /// connection error.
    pub async fn receive(&mut self) -> Result<String, SessionError> {
        let mut message = Vec::new();
        loop {
            let frame = self.read_frame().await?;
            match frame.opcode {
                0x1 | 0x2 => {
                    message.extend_from_slice(&frame.payload);
                    if frame.fin {
                        return Ok(
                            String::from_utf8_lossy(&message).into_owned()
                        );
                    }
                }
                0x9 => {
                    self.send_control(0xA, &frame.payload).await?;
                }
                0x8 => {
                    return Err(SessionError::Connection(
                        "websocket closed by device".to_string(),
                    ));
                }
                _ => {}
            }
        }
    }

    async fn read_frame(&mut self) -> Result<Frame, SessionError> {
        read_frame(&mut self.reader)
            .await
            .map_err(|e| SessionError::Connection(e.0))
    }

    async fn send_control(
        &mut self,
        opcode: u8,
        payload: &[u8],
    ) -> Result<(), SessionError> {
        let mut frame = vec![0x80 | opcode];
        frame.push(0x80 | payload.len() as u8);
        let mask = random_mask();
        frame.extend_from_slice(&mask);
        for (i, byte) in payload.iter().enumerate() {
            frame.push(byte ^ mask[i % 4]);
        }
        self.writer
            .write_all(&frame)
            .await
            .map_err(|e| SessionError::Connection(e.to_string()))
    }
}

struct Frame {
    fin: bool,
    opcode: u8,
    payload: Vec<u8>,
}

async fn read_frame(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
) -> Result<Frame, WsError> {
    let header = read_exact_n(reader, 2).await?;
    let fin = header[0] & 0x80 != 0;
    let opcode = header[0] & 0x0F;
    let masked = header[1] & 0x80 != 0;
    let length = match header[1] & 0x7F {
        126 => u64::from(u16::from_be_bytes(
            read_exact_n(reader, 2).await?[..]
                .try_into()
                .unwrap_or([0, 0]),
        )),
        127 => u64::from_be_bytes(
            read_exact_n(reader, 8).await?[..]
                .try_into()
                .unwrap_or([0; 8]),
        ),
        n => u64::from(n),
    };
    if length > 16 * 1024 * 1024 {
        return Err(WsError("websocket frame too large".to_string()));
    }
    let mask = if masked {
        Some(read_exact_n(reader, 4).await?)
    } else {
        None
    };
    let mut payload = read_exact_n(reader, length as usize).await?;
    if let Some(mask) = mask {
        for (i, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[i % 4];
        }
    }
    Ok(Frame {
        fin,
        opcode,
        payload,
    })
}

async fn read_exact_n(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
    n: usize,
) -> Result<Vec<u8>, WsError> {
    let mut buf = vec![0u8; n];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(|e| WsError(e.to_string()))?;
    Ok(buf)
}

async fn handshake(
    mut stream: TcpStream,
    host: &str,
    port: u16,
    origin: Option<&str>,
) -> Result<WsClient, WsError> {
    let key = random_key();
    let mut req = format!("GET / HTTP/1.1\r\nHost: {host}:{port}\r\n");
    req.push_str("Upgrade: websocket\r\nConnection: Upgrade\r\n");
    req.push_str(&format!("Sec-WebSocket-Key: {key}\r\n"));
    req.push_str("Sec-WebSocket-Version: 13\r\n");
    if let Some(origin) = origin {
        req.push_str(&format!("Origin: {origin}\r\n"));
    }
    req.push_str("\r\n");
    stream.write_all(req.as_bytes()).await?;
    stream.flush().await?;

    let mut response = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            return Err(WsError(
                "connection closed during websocket handshake".to_string(),
            ));
        }
        response.push(byte[0]);
        if response.ends_with(b"\r\n\r\n") {
            break;
        }
        if response.len() > 16 * 1024 {
            return Err(WsError(
                "oversized websocket handshake response".to_string(),
            ));
        }
    }
    let headers = String::from_utf8_lossy(&response);
    let expected = accept_key(&key);
    let confirmed = headers.lines().any(|line| {
        let Some((name, value)) = line.split_once(':') else {
            return false;
        };
        name.trim().eq_ignore_ascii_case("sec-websocket-accept")
            && value.trim() == expected
    });
    if !confirmed {
        return Err(WsError(
            "device did not complete the websocket handshake".to_string(),
        ));
    }
    let (reader, writer) = stream.into_split();
    Ok(WsClient {
        reader: BufReader::new(reader),
        writer,
    })
}

/// Compute the `Sec-WebSocket-Accept` reply for a handshake key
/// (RFC 6455 fixes the derivation to SHA-1 + base64).
fn accept_key(key: &str) -> String {
    use base64::Engine as _;
    use sha1::{Digest, Sha1};

    let mut hasher = Sha1::new();
    hasher.update(format!("{key}{WS_GUID}").as_bytes());
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}

/// Deterministic random-looking bytes from the system clock; only
/// used for WebSocket masking, where uniqueness (not secrecy)
/// matters.
fn random_mask() -> [u8; 4] {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    nanos.to_be_bytes()
}

fn random_key() -> String {
    use base64::Engine as _;

    let mut bytes = [0u8; 16];
    for (i, chunk) in bytes.chunks_mut(4).enumerate() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos()
            ^ (i as u32).wrapping_mul(0x9E3779B9);
        chunk.copy_from_slice(&nanos.to_be_bytes()[..chunk.len()]);
    }
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
