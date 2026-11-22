//! Client-side remote session proxy.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{ReadHalf, WriteHalf};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::core::discovery::daemon::{
    connect_stream, is_loopback, BoxedStream,
};
use crate::core::remote::inspect;
use crate::core::remote::protocol::{self, WireEvent};
use crate::core::remote::registry;
use crate::core::types::DeviceState;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct RemoteEndpoint {
    pub host: String,
    pub port: u16,
    pub token: Option<String>,
    pub tls_ca: Option<String>,
    pub tls_hostname: Option<String>,
}

/// Receives events from the remote session.
pub trait RemoteEvents: Send + Sync {
    fn event(&self, event: WireEvent);
    fn progress(&self, op: i64);
}

pub struct RemoteSession {
    endpoint: RemoteEndpoint,
    protocol: Mutex<Option<String>>,
    config: Value,
    dialect: Mutex<Value>,
    device: Option<String>,
    session_id: Mutex<Option<String>>,
    metadata: Mutex<Option<Value>>,
    state: Mutex<DeviceState>,
    job_running: AtomicBool,
    resource_uri: Mutex<Option<String>>,
    buffer_count: AtomicI64,
    rx_buffer_size: AtomicI64,
    pending_commands: Mutex<Vec<String>>,
    next_id: AtomicI64,
    pending: Mutex<HashMap<i64, oneshot::Sender<Value>>>,
    outgoing: Mutex<Option<mpsc::UnboundedSender<Value>>>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    last_seq: AtomicI64,
    connected: AtomicBool,
    closing: AtomicBool,
    connect_lock: tokio::sync::Mutex<()>,
    events: Arc<dyn RemoteEvents>,
    job_done: Mutex<Option<oneshot::Sender<Result<(), String>>>>,
}

impl RemoteSession {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        protocol_name: Option<String>,
        config: Value,
        dialect: Option<Value>,
        endpoint: RemoteEndpoint,
        device: Option<String>,
        session_id: Option<String>,
        events: Arc<dyn RemoteEvents>,
    ) -> Arc<Self> {
        Arc::new(Self {
            endpoint,
            protocol: Mutex::new(protocol_name),
            config,
            dialect: Mutex::new(dialect.unwrap_or_else(|| json!({}))),
            device,
            session_id: Mutex::new(session_id),
            metadata: Mutex::new(None),
            state: Mutex::new(DeviceState::default()),
            job_running: AtomicBool::new(false),
            resource_uri: Mutex::new(None),
            buffer_count: AtomicI64::new(0),
            rx_buffer_size: AtomicI64::new(0),
            pending_commands: Mutex::new(Vec::new()),
            next_id: AtomicI64::new(0),
            pending: Mutex::new(HashMap::new()),
            outgoing: Mutex::new(None),
            tasks: Mutex::new(Vec::new()),
            last_seq: AtomicI64::new(0),
            connected: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            connect_lock: tokio::sync::Mutex::new(()),
            events,
            job_done: Mutex::new(None),
        })
    }

    // -- cached properties -------------------------------------------

    pub fn protocol(&self) -> Option<String> {
        self.protocol.lock().unwrap().clone()
    }

    pub fn metadata(&self) -> Option<Value> {
        self.metadata.lock().unwrap().clone()
    }

    pub fn state(&self) -> DeviceState {
        self.state.lock().unwrap().clone()
    }

    pub fn job_running(&self) -> bool {
        self.job_running.load(Ordering::SeqCst)
    }

    pub fn resource_uri(&self) -> Option<String> {
        self.resource_uri.lock().unwrap().clone()
    }

    pub fn buffer_count(&self) -> i64 {
        self.buffer_count.load(Ordering::SeqCst)
    }

    pub fn rx_buffer_size(&self) -> i64 {
        self.rx_buffer_size.load(Ordering::SeqCst)
    }

    pub fn pending_commands(&self) -> Vec<String> {
        self.pending_commands.lock().unwrap().clone()
    }

    pub fn endpoint(&self) -> &RemoteEndpoint {
        &self.endpoint
    }

    /// Drop the connection as if the network failed, failing pending
    /// requests and emitting a SLEEPING connection status.
    pub fn force_disconnect(&self, message: &str) {
        self.connected.store(true, Ordering::SeqCst);
        self.on_disconnect(message);
        self.teardown();
    }

    // -- connection lifecycle -----------------------------------------

    pub async fn ensure_connected(self: &Arc<Self>) -> Result<(), String> {
        if self.connected.load(Ordering::SeqCst) {
            return Ok(());
        }
        let _guard = self.connect_lock.lock().await;
        if self.connected.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.open_connection().await
    }

    pub async fn attach(self: &Arc<Self>) -> Result<(), String> {
        self.ensure_connected().await
    }

    async fn open_connection(self: &Arc<Self>) -> Result<(), String> {
        let endpoint = &self.endpoint;
        if endpoint.token.is_some()
            && endpoint.tls_ca.is_none()
            && !is_loopback(&endpoint.host)
        {
            return Err("refusing to send a token without TLS".to_string());
        }
        let mut stream = connect_stream(
            &endpoint.host,
            endpoint.port,
            endpoint.tls_ca.as_deref(),
            endpoint.tls_hostname.as_deref(),
            CONNECT_TIMEOUT,
        )
        .await?;
        let welcome = protocol::read_frame(&mut stream)
            .await
            .map_err(|err| err.to_string())?;
        if welcome.get("type").and_then(Value::as_str)
            != Some(protocol::WELCOME)
        {
            return Err("daemon did not send welcome".to_string());
        }
        let payload = welcome.get("payload").cloned().unwrap_or(Value::Null);
        if payload.get("protocol").and_then(Value::as_i64)
            != Some(protocol::PROTOCOL_VERSION)
        {
            return Err(format!(
                "unsupported daemon protocol version {:?}",
                payload.get("protocol")
            ));
        }
        if payload
            .get("auth_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            let auth = protocol::request(
                0,
                protocol::AUTH,
                json!({"token": endpoint.token.clone().unwrap_or_default()}),
            );
            protocol::write_frame(&mut stream, &auth)
                .await
                .map_err(|err| err.to_string())?;
            let reply = protocol::read_frame(&mut stream)
                .await
                .map_err(|err| err.to_string())?;
            if !reply.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                return Err("daemon authentication failed".to_string());
            }
        }

        let (reader, writer) = tokio::io::split(stream);
        let (outgoing, outgoing_rx) = mpsc::unbounded_channel();
        *self.outgoing.lock().unwrap() = Some(outgoing);
        self.closing.store(false, Ordering::SeqCst);

        let reader_session = self.clone();
        let reader_task = tokio::spawn(async move {
            reader_session.reader_loop(reader).await;
        });
        let writer_session = self.clone();
        let writer_task = tokio::spawn(async move {
            writer_session.writer_loop(writer, outgoing_rx).await;
        });
        self.tasks.lock().unwrap().push(reader_task);
        self.tasks.lock().unwrap().push(writer_task);

        let has_session = self.session_id.lock().unwrap().is_some();
        if !has_session {
            let payload = if let Some(device) = &self.device {
                json!({
                    "device": device,
                    "dialect": self.dialect.lock().unwrap().clone(),
                    "role": "control",
                })
            } else {
                json!({
                    "protocol": self.protocol.lock().unwrap().clone(),
                    "config": self.config,
                    "dialect": self.dialect.lock().unwrap().clone(),
                    "role": "control",
                })
            };
            let reply = self.request(protocol::SESSION_OPEN, payload).await?;
            check_reply(&reply)?;
            let payload = reply.get("payload").cloned().unwrap_or(Value::Null);
            *self.session_id.lock().unwrap() = payload
                .get("session_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(name) = payload.get("protocol").and_then(Value::as_str)
            {
                *self.protocol.lock().unwrap() = Some(name.to_string());
            }
            *self.metadata.lock().unwrap() = payload.get("metadata").cloned();
        } else {
            self.reattach().await?;
        }
        self.connected.store(true, Ordering::SeqCst);
        self.refresh_properties().await?;
        Ok(())
    }

    async fn reattach(&self) -> Result<(), String> {
        let session_id = self
            .session_id
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "no session to attach".to_string())?;
        let mut reply = self
            .request(
                protocol::SESSION_ATTACH,
                json!({
                    "session_id": session_id,
                    "after_seq": self.last_seq.load(Ordering::SeqCst),
                    "role": "control",
                }),
            )
            .await?;
        // A just-dropped connection may not have been noticed by the
        // daemon yet. Retry briefly: the daemon starts the lease
        // grace period when it sees the old socket close.
        for _ in 0..40 {
            if reply.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                break;
            }
            let code = reply
                .get("error")
                .and_then(|error| error.get("code"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if code != protocol::LEASE_TAKEN {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            reply = self
                .request(
                    protocol::SESSION_ATTACH,
                    json!({
                        "session_id": session_id,
                        "after_seq":
                            self.last_seq.load(Ordering::SeqCst),
                        "role": "control",
                    }),
                )
                .await?;
        }
        check_reply(&reply)?;
        if let Some(snapshot) = reply
            .get("payload")
            .and_then(|payload| payload.get("snapshot"))
        {
            self.apply_snapshot(snapshot);
        }
        Ok(())
    }

    fn apply_snapshot(&self, snapshot: &Value) {
        if let Some(name) = snapshot.get("protocol").and_then(Value::as_str) {
            *self.protocol.lock().unwrap() = Some(name.to_string());
            *self.metadata.lock().unwrap() =
                registry::protocol_info(name).map(|info| info.to_json());
        }
        if let Some(state) = snapshot.get("state") {
            if let Ok(state) = protocol::decode_state(state) {
                *self.state.lock().unwrap() = state.clone();
                self.emit(WireEvent::State(state));
            }
        }
        let next_seq = snapshot
            .get("next_seq")
            .and_then(Value::as_i64)
            .unwrap_or(1);
        self.last_seq.store(next_seq - 1, Ordering::SeqCst);
        self.job_running.store(
            snapshot
                .get("job")
                .and_then(|job| job.get("running"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            Ordering::SeqCst,
        );
        if let Some(status) =
            snapshot.get("connection_status").and_then(Value::as_str)
        {
            self.emit(WireEvent::ConnectionStatus {
                status: status.to_string(),
                message: None,
            });
        }
    }

    async fn refresh_properties(&self) -> Result<(), String> {
        for name in [
            "resource_uri",
            "buffer_count",
            "rx_buffer_size",
            "pending_commands",
        ] {
            let reply = self
                .request(
                    protocol::SESSION_PROPERTY,
                    json!({
                        "session_id":
                            self.session_id.lock().unwrap().clone(),
                        "name": name,
                    }),
                )
                .await?;
            if !reply.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                continue;
            }
            let value = reply
                .get("payload")
                .and_then(|payload| payload.get("value"))
                .cloned()
                .unwrap_or(Value::Null);
            match name {
                "resource_uri" => {
                    *self.resource_uri.lock().unwrap() =
                        value.as_str().map(str::to_string)
                }
                "buffer_count" => self
                    .buffer_count
                    .store(value.as_i64().unwrap_or(0), Ordering::SeqCst),
                "rx_buffer_size" => self
                    .rx_buffer_size
                    .store(value.as_i64().unwrap_or(0), Ordering::SeqCst),
                "pending_commands" => {
                    *self.pending_commands.lock().unwrap() = value
                        .as_array()
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default()
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub async fn aclose(self: &Arc<Self>) {
        self.closing.store(true, Ordering::SeqCst);
        if self.connected.load(Ordering::SeqCst) {
            let session_id = self.session_id.lock().unwrap().clone();
            if let Some(session_id) = session_id {
                let _ = self
                    .request(
                        protocol::SESSION_DETACH,
                        json!({"session_id": session_id}),
                    )
                    .await;
            }
        }
        self.teardown();
    }

    fn teardown(&self) {
        self.connected.store(false, Ordering::SeqCst);
        let tasks = std::mem::take(&mut *self.tasks.lock().unwrap());
        for task in tasks {
            task.abort();
        }
        *self.outgoing.lock().unwrap() = None;
    }

    // -- wire plumbing --------------------------------------------------

    fn enqueue(&self, frame: Value) -> Result<(), String> {
        let outgoing = self.outgoing.lock().unwrap();
        let Some(outgoing) = outgoing.as_ref() else {
            return Err("connection lost".to_string());
        };
        outgoing
            .send(frame)
            .map_err(|_| "connection lost".to_string())
    }

    async fn request(
        &self,
        type_: &str,
        payload: Value,
    ) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        if let Err(err) = self.enqueue(protocol::request(id, type_, payload)) {
            self.pending.lock().unwrap().remove(&id);
            return Err(err);
        }
        rx.await.map_err(|_| "connection lost".to_string())
    }

    fn enqueue_without_reply(
        &self,
        type_: &str,
        payload: Value,
    ) -> Result<(), String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        self.enqueue(protocol::request(id, type_, payload))
    }

    async fn writer_loop(
        self: Arc<Self>,
        mut writer: WriteHalf<BoxedStream>,
        mut outgoing: mpsc::UnboundedReceiver<Value>,
    ) {
        while let Some(frame) = outgoing.recv().await {
            if let Err(err) = protocol::write_frame(&mut writer, &frame).await {
                self.on_disconnect(&format!("connection lost: {err}"));
                return;
            }
        }
    }

    async fn reader_loop(self: Arc<Self>, mut reader: ReadHalf<BoxedStream>) {
        loop {
            let message = match protocol::read_frame(&mut reader).await {
                Ok(message) => message,
                Err(err) => {
                    self.on_disconnect(&err.to_string());
                    return;
                }
            };
            match message.get("type").and_then(Value::as_str) {
                Some(protocol::PING) => {
                    let _ = self.enqueue(json!({"type": protocol::PONG}));
                }
                Some(protocol::EVENT) => {
                    if let Some(payload) = message.get("payload") {
                        self.handle_event(payload);
                    }
                }
                Some(protocol::PONG) => {}
                _ => {
                    let id = message.get("id").and_then(Value::as_i64);
                    if let Some(id) = id {
                        if let Some(sender) =
                            self.pending.lock().unwrap().remove(&id)
                        {
                            let _ = sender.send(message);
                        }
                    }
                }
            }
        }
    }

    fn handle_event(&self, payload: &Value) {
        let seq = payload.get("seq").and_then(Value::as_i64).unwrap_or(0);
        let last = self.last_seq.load(Ordering::SeqCst);
        if seq > last {
            self.last_seq.store(seq, Ordering::SeqCst);
        }
        let Some(event) = payload.get("event") else {
            return;
        };
        if event.get("kind").and_then(Value::as_str)
            == Some(protocol::KIND_PROGRESS)
        {
            let op = event.get("op").and_then(Value::as_i64).unwrap_or(0);
            self.events.progress(op);
            return;
        }
        if event.get("kind").and_then(Value::as_str)
            == Some(protocol::KIND_JOB_ERROR)
        {
            self.job_running.store(false, Ordering::SeqCst);
            self.finish_job(Ok(()));
            return;
        }
        if event.get("kind").and_then(Value::as_str)
            == Some(protocol::KIND_LEASE)
        {
            return;
        }
        let Some(decoded) = WireEvent::from_json(event) else {
            return;
        };
        match &decoded {
            WireEvent::State(state) => {
                *self.state.lock().unwrap() = state.clone();
            }
            WireEvent::JobFinished => {
                self.job_running.store(false, Ordering::SeqCst);
                self.finish_job(Ok(()));
            }
            _ => {}
        }
        self.emit(decoded);
    }

    fn emit(&self, event: WireEvent) {
        self.events.event(event);
    }

    fn finish_job(&self, result: Result<(), String>) {
        if let Some(sender) = self.job_done.lock().unwrap().take() {
            let _ = sender.send(result);
        }
    }

    fn on_disconnect(&self, message: &str) {
        if self.closing.load(Ordering::SeqCst)
            || !self.connected.swap(false, Ordering::SeqCst)
        {
            return;
        }
        let pending = std::mem::take(&mut *self.pending.lock().unwrap());
        for sender in pending.into_values() {
            let _ = sender.send(json!({
                "ok": false,
                "error": {"code": "internal", "message": message},
            }));
        }
        self.finish_job(Err(message.to_string()));
        self.emit(WireEvent::ConnectionStatus {
            status: "SLEEPING".to_string(),
            message: Some(message.to_string()),
        });
    }

    // -- command surface -------------------------------------------------

    pub(crate) async fn command(
        self: &Arc<Self>,
        method: &str,
        args: Value,
    ) -> Result<Value, String> {
        self.ensure_connected().await?;
        let reply = self
            .request(
                protocol::SESSION_COMMAND,
                json!({
                    "session_id":
                        self.session_id.lock().unwrap().clone(),
                    "method": method,
                    "args": args,
                }),
            )
            .await?;
        check_reply(&reply)?;
        Ok(reply
            .get("payload")
            .and_then(|payload| payload.get("result"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    pub async fn connect(self: &Arc<Self>) -> Result<(), String> {
        self.command("connect", json!({})).await.map(|_| ())
    }

    pub async fn disconnect(self: &Arc<Self>) -> Result<(), String> {
        self.aclose().await;
        Ok(())
    }

    pub async fn execute_interactive_command(
        self: &Arc<Self>,
        command: &str,
    ) -> Result<Value, String> {
        self.command("execute_interactive_command", json!({"command": command}))
            .await
    }

    pub async fn execute_command(
        self: &Arc<Self>,
        command: &str,
    ) -> Result<Value, String> {
        self.command("execute_command", json!({"command": command}))
            .await
    }

    pub async fn run(
        self: &Arc<Self>,
        gcode: &str,
        op_map: Option<HashMap<usize, i64>>,
        op_estimates: Option<Vec<f64>>,
    ) -> Result<(), String> {
        self.ensure_connected().await?;
        let (tx, rx) = oneshot::channel();
        *self.job_done.lock().unwrap() = Some(tx);
        let mut args = json!({"gcode": gcode});
        if let Some(op_map) = &op_map {
            let pairs: Vec<Value> =
                op_map.iter().map(|(line, op)| json!([line, op])).collect();
            args["op_line_map"] = json!(pairs);
        }
        if let Some(estimates) = &op_estimates {
            args["op_estimates"] = json!(estimates);
        }
        let reply = self
            .request(
                protocol::SESSION_COMMAND,
                json!({
                    "session_id":
                        self.session_id.lock().unwrap().clone(),
                    "method": "run",
                    "args": args,
                }),
            )
            .await;
        let reply = match reply {
            Ok(reply) => reply,
            Err(err) => {
                *self.job_done.lock().unwrap() = None;
                return Err(err);
            }
        };
        if !reply.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            *self.job_done.lock().unwrap() = None;
            return check_reply(&reply);
        }
        self.job_running.store(true, Ordering::SeqCst);
        rx.await.map_err(|_| "connection lost".to_string())??;
        Ok(())
    }

    pub async fn run_raw(
        self: &Arc<Self>,
        machine_code: &str,
    ) -> Result<(), String> {
        self.command("run_raw", json!({"machine_code": machine_code}))
            .await
            .map(|_| ())
    }

    pub async fn set_hold(self: &Arc<Self>, hold: bool) -> Result<(), String> {
        self.command("set_hold", json!({"hold": hold}))
            .await
            .map(|_| ())
    }

    pub async fn cancel(
        self: &Arc<Self>,
        emergency: bool,
    ) -> Result<(), String> {
        self.command("cancel", json!({"emergency": emergency}))
            .await
            .map(|_| ())
    }

    pub async fn home(
        self: &Arc<Self>,
        axes: Option<Vec<String>>,
        active_wcs: &str,
    ) -> Result<(), String> {
        self.command("home", json!({"axes": axes, "active_wcs": active_wcs}))
            .await
            .map(|_| ())
    }

    pub async fn move_to(
        self: &Arc<Self>,
        speed: f64,
        x: f64,
        y: f64,
        z: Option<f64>,
    ) -> Result<(), String> {
        self.command("move_to", json!({"speed": speed, "x": x, "y": y, "z": z}))
            .await
            .map(|_| ())
    }

    pub async fn jog(
        self: &Arc<Self>,
        speed: f64,
        deltas: Value,
    ) -> Result<(), String> {
        self.command("jog", json!({"speed": speed, "deltas": deltas}))
            .await
            .map(|_| ())
    }

    pub async fn select_tool(
        self: &Arc<Self>,
        tool_number: i64,
    ) -> Result<(), String> {
        self.command("select_tool", json!({"tool_number": tool_number}))
            .await
            .map(|_| ())
    }

    pub async fn set_power(
        self: &Arc<Self>,
        power: Option<f64>,
    ) -> Result<(), String> {
        self.command("set_power", json!({"power": power}))
            .await
            .map(|_| ())
    }

    pub async fn set_focus_power(
        self: &Arc<Self>,
        power: Option<f64>,
    ) -> Result<(), String> {
        self.command("set_focus_power", json!({"power": power}))
            .await
            .map(|_| ())
    }

    pub async fn read_settings(self: &Arc<Self>) -> Result<Value, String> {
        self.command("read_settings", json!({})).await
    }

    pub async fn write_setting(
        self: &Arc<Self>,
        key: &str,
        value: &str,
    ) -> Result<(), String> {
        self.command("write_setting", json!({"key": key, "value": value}))
            .await
            .map(|_| ())
    }

    pub async fn set_wcs_offset(
        self: &Arc<Self>,
        wcs_slot: &str,
        x: f64,
        y: f64,
        z: Option<f64>,
    ) -> Result<(), String> {
        self.command(
            "set_wcs_offset",
            json!({"wcs_slot": wcs_slot, "x": x, "y": y, "z": z}),
        )
        .await
        .map(|_| ())
    }

    pub async fn read_wcs_offsets(self: &Arc<Self>) -> Result<Value, String> {
        self.command("read_wcs_offsets", json!({})).await
    }

    pub async fn read_parser_state(self: &Arc<Self>) -> Result<Value, String> {
        self.command("read_parser_state", json!({})).await
    }

    pub async fn run_probe_cycle(
        self: &Arc<Self>,
        axis_letter: &str,
        max_travel: f64,
        feed_rate: f64,
    ) -> Result<Value, String> {
        self.command(
            "run_probe_cycle",
            json!({
                "axis_letter": axis_letter,
                "max_travel": max_travel,
                "feed_rate": feed_rate,
            }),
        )
        .await
    }

    pub async fn detect_unit_system(self: &Arc<Self>) -> Result<Value, String> {
        self.command("detect_unit_system", json!({})).await
    }

    pub async fn clear_alarm(self: &Arc<Self>) -> Result<(), String> {
        self.command("clear_alarm", json!({})).await.map(|_| ())
    }

    /// Applies a dialect to the hosted session. Callable from sync
    /// contexts (no runtime): while connected the command is enqueued
    /// onto the socket thread; before connecting the dialect is only
    /// stored, and the handshake carries it to the daemon.
    pub fn update_dialect(self: &Arc<Self>, dialect: Value) {
        *self.dialect.lock().unwrap() = dialect.clone();
        if self.connected.load(Ordering::SeqCst) {
            let _ = self.enqueue_without_reply(
                protocol::SESSION_COMMAND,
                json!({
                    "session_id":
                        self.session_id.lock().unwrap().clone(),
                    "method": "update_dialect",
                    "args": {"dialect": dialect},
                }),
            );
        }
    }
}

fn check_reply(reply: &Value) -> Result<(), String> {
    if reply.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(());
    }
    Err(reply
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("remote command failed")
        .to_string())
}

/// The devices a daemon knows about.
pub async fn list_devices(
    endpoint: &RemoteEndpoint,
) -> Result<Vec<Value>, String> {
    let requests = [(protocol::DEVICE_LIST, json!({}))];
    let (_, replies) = inspect::daemon_rpc(
        &endpoint.host,
        endpoint.port,
        endpoint.token.as_deref(),
        endpoint.tls_ca.as_deref(),
        endpoint.tls_hostname.as_deref(),
        &requests,
    )
    .await?;
    let reply = &replies[0];
    if !reply.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return Err(reply
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("device/list failed")
            .to_string());
    }
    Ok(reply
        .get("payload")
        .and_then(|payload| payload.get("devices"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// The daemon's protocol metadata.
pub async fn list_protocols(
    endpoint: &RemoteEndpoint,
) -> Result<Vec<Value>, String> {
    let requests = [(protocol::PROTOCOL_LIST, json!({}))];
    let (_, replies) = inspect::daemon_rpc(
        &endpoint.host,
        endpoint.port,
        endpoint.token.as_deref(),
        endpoint.tls_ca.as_deref(),
        endpoint.tls_hostname.as_deref(),
        &requests,
    )
    .await?;
    let reply = &replies[0];
    if !reply.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return Err("protocol/list failed".to_string());
    }
    Ok(reply
        .get("payload")
        .and_then(|payload| payload.get("protocols"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}
