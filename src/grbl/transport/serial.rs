//! Async serial port transport backed by `tokio-serial`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_serial::{SerialPortBuilderExt, SerialStream};

use super::{Transport, TransportError};

pub struct SerialTransport {
    port: String,
    baudrate: u32,
    open: AtomicBool,
    writer: Mutex<Option<WriteHalf<SerialStream>>>,
    reader_task: Mutex<Option<JoinHandle<()>>>,
}

impl SerialTransport {
    pub fn new(port: &str, baudrate: u32) -> Arc<Self> {
        Arc::new(Self {
            port: port.to_string(),
            baudrate,
            open: AtomicBool::new(false),
            writer: Mutex::new(None),
            reader_task: Mutex::new(None),
        })
    }
}

#[async_trait]
impl Transport for SerialTransport {
    async fn open(
        &self,
        tx: mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<(), TransportError> {
        let builder = tokio_serial::new(&self.port, self.baudrate);
        let stream: SerialStream = builder
            .open_native_async()
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
        Some(format!("serial://{}", self.port))
    }
}

async fn reader_loop(
    mut read_half: ReadHalf<SerialStream>,
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
                log::debug!("Serial read error: {e}");
                break;
            }
        }
    }
}
