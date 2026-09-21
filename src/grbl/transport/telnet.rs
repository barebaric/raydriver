//! Raw TCP transport (telnet-style) for network-attached Grbl
//! devices.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use super::{Transport, TransportError};

pub struct TelnetTransport {
    host: String,
    port: u16,
    open: AtomicBool,
    writer: Mutex<Option<WriteHalf<TcpStream>>>,
    reader_task: Mutex<Option<JoinHandle<()>>>,
}

impl TelnetTransport {
    pub fn new(host: &str, port: u16) -> Arc<Self> {
        Arc::new(Self {
            host: host.to_string(),
            port,
            open: AtomicBool::new(false),
            writer: Mutex::new(None),
            reader_task: Mutex::new(None),
        })
    }
}

#[async_trait]
impl Transport for TelnetTransport {
    async fn open(
        &self,
        tx: mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<(), TransportError> {
        let stream = TcpStream::connect((self.host.as_str(), self.port))
            .await
            .map_err(|e| TransportError::Connection(e.to_string()))?;
        let (read_half, write_half) = tokio::io::split(stream);
        *self.writer.lock().await = Some(write_half);
        let task = tokio::spawn(reader_loop(read_half, tx));
        *self.reader_task.lock().await = Some(task);
        self.open.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn close(&self) {
        self.open.store(false, Ordering::SeqCst);
        if let Some(task) = self.reader_task.lock().await.take() {
            task.abort();
        }
        *self.writer.lock().await = None;
    }

    async fn write(&self, data: &[u8]) -> Result<(), TransportError> {
        let mut writer_guard = self.writer.lock().await;
        let Some(writer) = writer_guard.as_mut() else {
            return Err(TransportError::NotConnected);
        };
        match writer.write_all(data).await {
            Ok(()) => {
                let _ = writer.flush().await;
                Ok(())
            }
            Err(e) => {
                self.open.store(false, Ordering::SeqCst);
                Err(TransportError::Connection(e.to_string()))
            }
        }
    }

    fn is_open(&self) -> bool {
        self.open.load(Ordering::SeqCst)
    }

    fn resource_uri(&self) -> Option<String> {
        Some(format!("tcp://{}:{}", self.host, self.port))
    }
}

async fn reader_loop(
    mut read_half: ReadHalf<TcpStream>,
    tx: mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut buf = vec![0u8; 1024];
    loop {
        match read_half.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                if tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
            Err(e) => {
                log::debug!("Telnet read error: {e}");
                break;
            }
        }
    }
}
