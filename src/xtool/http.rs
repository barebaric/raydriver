//! Minimal HTTP/1.1 client for the xTool WiFi API.
//!
//! Hand-rolled over a tokio `TcpStream` so the crate stays free of
//! an HTTP client dependency: the API this module serves is small
//! and fixed (short JSON GET/POST replies, one multipart upload
//! endpoint).  Requests are always sent with `Connection: close` and
//! the response is read to EOF, which keeps the parser simple; both
//! `Content-Length` and `Transfer-Encoding: chunked` bodies are
//! decoded.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::core::session::SessionError;

/// A parsed HTTP response.
#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// Perform a GET request.
pub async fn get(
    host: &str,
    port: u16,
    path: &str,
    timeout: Duration,
) -> Result<HttpResponse, SessionError> {
    request(host, port, "GET", path, None, None, timeout).await
}

/// Perform a POST request with a plain-text body.
pub async fn post(
    host: &str,
    port: u16,
    path: &str,
    content_type: &str,
    body: &[u8],
    timeout: Duration,
) -> Result<HttpResponse, SessionError> {
    request(
        host,
        port,
        "POST",
        path,
        Some((content_type, body)),
        None,
        timeout,
    )
    .await
}

/// Perform a POST request with a `multipart/form-data` body carrying
/// a single file field.
pub async fn post_multipart(
    host: &str,
    port: u16,
    path: &str,
    field: &str,
    filename: &str,
    file: &[u8],
    timeout: Duration,
) -> Result<HttpResponse, SessionError> {
    let boundary = format!("raydriver{}", std::process::id());
    let mut body = Vec::with_capacity(file.len() + 256);
    body.extend_from_slice(
        format!(
            "--{b}\r\n\
             Content-Disposition: form-data; name=\"{field}\"; \
             filename=\"{filename}\"\r\n\
             Content-Type: text/plain\r\n\
             \r\n",
            b = boundary,
            field = field,
            filename = filename,
        )
        .as_bytes(),
    );
    body.extend_from_slice(file);
    body.extend_from_slice(format!("\r\n--{b}--\r\n", b = boundary).as_bytes());
    let content_type = format!("multipart/form-data; boundary={boundary}");
    request(
        host,
        port,
        "POST",
        path,
        Some((&content_type, &body)),
        None,
        timeout,
    )
    .await
}

async fn request(
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    body: Option<(&str, &[u8])>,
    _extra: Option<()>,
    timeout: Duration,
) -> Result<HttpResponse, SessionError> {
    tokio::time::timeout(timeout, async {
        let stream = TcpStream::connect((host, port))
            .await
            .map_err(|e| SessionError::Connection(e.to_string()))?;
        exchange(stream, host, port, method, path, body)
            .await
            .map_err(|e| SessionError::Connection(e.to_string()))
    })
    .await
    .map_err(|_| SessionError::Timeout)?
}

async fn exchange(
    mut stream: TcpStream,
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    body: Option<(&str, &[u8])>,
) -> Result<HttpResponse, std::io::Error> {
    let mut req = format!("{method} {path} HTTP/1.1\r\n");
    req.push_str(&format!("Host: {host}:{port}\r\n"));
    req.push_str("Accept: */*\r\n");
    req.push_str("Connection: close\r\n");
    match body {
        Some((content_type, payload)) => {
            req.push_str(&format!("Content-Type: {content_type}\r\n"));
            req.push_str(&format!("Content-Length: {}\r\n", payload.len()));
        }
        None => {
            req.push_str("Content-Length: 0\r\n");
        }
    }
    req.push_str("\r\n");
    stream.write_all(req.as_bytes()).await?;
    if let Some((_, payload)) = body {
        stream.write_all(payload).await?;
    }
    stream.flush().await?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await?;
    parse_response(&raw)
}

fn parse_response(raw: &[u8]) -> Result<HttpResponse, std::io::Error> {
    let split = find_header_end(raw).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "malformed HTTP response: no header terminator",
        )
    })?;
    let (header_bytes, body_bytes) = raw.split_at(split);
    let headers = String::from_utf8_lossy(header_bytes);
    let mut lines = headers.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("malformed HTTP status line: {status_line:?}"),
            )
        })?;

    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_uppercase();
        let value = value.trim();
        match name.as_str() {
            "CONTENT-LENGTH" => {
                content_length = value.parse::<usize>().ok();
            }
            "TRANSFER-ENCODING" => {
                chunked = value.to_ascii_lowercase().contains("chunked");
            }
            _ => {}
        }
    }

    let body = if chunked {
        decode_chunked(body_bytes)?
    } else if let Some(length) = content_length {
        body_bytes[..length.min(body_bytes.len())].to_vec()
    } else {
        body_bytes.to_vec()
    };
    Ok(HttpResponse { status, body })
}

/// The number of bytes up to and including the `\r\n\r\n` header
/// terminator.
fn find_header_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

fn decode_chunked(mut body: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut out = Vec::new();
    while let Some(line_end) = find_crlf(body) {
        let size_line = String::from_utf8_lossy(&body[..line_end]);
        let size_str = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_str, 16).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("malformed chunk size: {size_str:?}"),
            )
        })?;
        body = &body[line_end + 2..];
        if size == 0 {
            break;
        }
        let take = size.min(body.len());
        out.extend_from_slice(&body[..take]);
        body = &body[(take + 2).min(body.len())..];
    }
    Ok(out)
}

fn find_crlf(raw: &[u8]) -> Option<usize> {
    raw.windows(2).position(|w| w == b"\r\n")
}
