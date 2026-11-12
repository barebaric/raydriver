"""Device-host daemon for remote sessions.

Hosts raydriver sessions next to the hardware and serves the
control protocol (:mod:`raydriver.remote.protocol`) over TCP,
optionally TLS. Jobs run daemon-side: a submitted job survives
client disconnects, and reattaching clients resync from the event
ring or a snapshot.

Security follows the analyzer's policy: loopback may run without
auth; non-loopback or wildcard binds require both a token and TLS.
The handshake is the protocol's JSON welcome/auth flow — the
analyzer's line-based token challenge is diagnostic-endpoint
specific and is not used here.
"""

from __future__ import annotations

import asyncio
import hmac
import importlib.metadata
import inspect
import logging
import time
import uuid
from collections import deque
from dataclasses import dataclass, field
from typing import Any, Callable

from raydriver.analyzer.listener import (
    InsecureListenerError,
    make_server_ssl_context,
    validate_bind_policy,
)
from raydriver.core import SessionEvent, list_serial_ports
from raydriver.grbl import GrblSession
from raydriver.grbl.simple import GrblSimpleSession
from raydriver.marlin import MarlinSession
from raydriver.remote import protocol
from raydriver.smoothie import SmoothieSession

logger = logging.getLogger(__name__)

DEFAULT_PORT = 8765
DEFAULT_RING_SIZE = 1000
DEFAULT_LEASE_GRACE = 5.0
SEND_QUEUE_LIMIT = 1000
AUTH_TIMEOUT = 10.0

SESSION_CLASSES: dict[str, Any] = {
    "grbl": GrblSession,
    "grbl_simple": GrblSimpleSession,
    "smoothie": SmoothieSession,
    "marlin": MarlinSession,
}


def _raydriver_version() -> str:
    try:
        return importlib.metadata.version("raydriver")
    except importlib.metadata.PackageNotFoundError:
        return "0.0.0"


def default_session_factory(
    protocol_name: str,
    config: dict[str, Any],
    dialect: dict[str, Any] | None,
    event_callback: Callable[[Any], None] | None,
) -> Any:
    """Construct a session exactly as the driver shells do."""
    cls = SESSION_CLASSES[protocol_name]
    return cls(config=config, dialect=dialect, event_callback=event_callback)


@dataclass
class DaemonConfig:
    """Listener and session-host settings (the daemon.json shape)."""

    host: str = "127.0.0.1"
    port: int = DEFAULT_PORT
    token: str | None = None
    tls_cert: str | None = None
    tls_key: str | None = None
    keepalive_interval: float = protocol.DEFAULT_KEEPALIVE_INTERVAL
    keepalive_misses: int = protocol.DEFAULT_KEEPALIVE_MISSES
    lease_grace: float = DEFAULT_LEASE_GRACE
    ring_size: int = DEFAULT_RING_SIZE
    cameras: list[dict[str, Any]] = field(default_factory=list)
    sessions: list[dict[str, Any]] = field(default_factory=list)


class ClientQueue:
    """Bounded per-client send buffer; drops raw events first."""

    def __init__(self, limit: int = SEND_QUEUE_LIMIT) -> None:
        self._limit = limit
        self._frames: deque[dict[str, Any]] = deque()
        self._ready = asyncio.Event()

    def put(self, frame: dict[str, Any]) -> None:
        if len(self._frames) >= self._limit:
            self._drop_raw()
        if len(self._frames) >= self._limit:
            raise OverflowError("client send queue overflow")
        self._frames.append(frame)
        self._ready.set()

    def _drop_raw(self) -> None:
        kept = [
            frame
            for frame in self._frames
            if frame.get("payload", {}).get("event", {}).get("kind")
            not in ("raw_rx", "raw_tx")
        ]
        self._frames = deque(kept)

    async def get(self) -> dict[str, Any]:
        while not self._frames:
            self._ready.clear()
            await self._ready.wait()
        return self._frames.popleft()


class Client:
    """One connected control client."""

    def __init__(
        self,
        reader: asyncio.StreamReader,
        writer: asyncio.StreamWriter,
    ) -> None:
        self.id = uuid.uuid4().hex[:12]
        self.reader = reader
        self.writer = writer
        self.queue = ClientQueue()
        self.authenticated = False
        self.closed = False
        self.last_rx = time.monotonic()
        self.roles: dict[str, str] = {}
        self.tasks: list[asyncio.Task] = []

    def send(self, frame: dict[str, Any]) -> None:
        self.queue.put(frame)


class SessionHost:
    """A hosted session: event ring, subscribers, lease and job."""

    def __init__(
        self,
        session_id: str,
        protocol_name: str,
        config: dict[str, Any],
        dialect: dict[str, Any],
        ring_size: int,
    ) -> None:
        self.id = session_id
        self.protocol_name = protocol_name
        self.config = config
        self.dialect = dialect
        self.session: Any = None
        self.ring: deque[tuple[int, dict[str, Any]]] = deque(maxlen=ring_size)
        self.next_seq = 1
        self.subscribers: dict[str, Client] = {}
        self.lease_holder: str | None = None
        self.lease_in_grace = False
        self.grace_task: asyncio.Task | None = None
        self.job: dict[str, Any] | None = None
        self.job_task: asyncio.Task | None = None
        self.connection_status = "DISCONNECTED"


class Daemon:
    """Session host serving the remote control protocol."""

    def __init__(
        self,
        config: DaemonConfig | None = None,
        session_factory: Callable[..., Any] | None = None,
    ) -> None:
        self.config = config or DaemonConfig()
        self._factory = session_factory or default_session_factory
        self._hosts: dict[str, SessionHost] = {}
        self._clients: dict[str, Client] = {}
        self._server: asyncio.Server | None = None
        self._handlers: dict[str, Any] = {
            protocol.DEVICE_LIST: self._device_list,
            protocol.SESSION_LIST: self._session_list,
            protocol.SESSION_OPEN: self._session_open,
            protocol.SESSION_ATTACH: self._session_attach,
            protocol.SESSION_DETACH: self._session_detach,
            protocol.SESSION_CLOSE: self._session_close,
            protocol.SESSION_COMMAND: self._session_command,
            protocol.SESSION_PROPERTY: self._session_property,
        }

    async def start(self) -> asyncio.Server:
        """Open the listener and auto-open configured sessions."""
        cfg = self.config
        validate_bind_policy(cfg.host, cfg.token, cfg.tls_cert)
        ssl_context = None
        if cfg.tls_cert is not None:
            if cfg.tls_key is None:
                raise InsecureListenerError("tls_cert also requires tls_key")
            ssl_context = make_server_ssl_context(cfg.tls_cert, cfg.tls_key)
        self._server = await asyncio.start_server(
            self._handle, cfg.host, cfg.port, ssl=ssl_context
        )
        for entry in cfg.sessions:
            self._open_host(
                entry.get("protocol", ""),
                entry.get("config") or {},
                entry.get("dialect") or {},
            )
        return self._server

    async def stop(self) -> None:
        """Close every session and connection, then the listener."""
        for host in list(self._hosts.values()):
            await self._close_host(host)
        for client in list(self._clients.values()):
            await self._disconnect(client)
        if self._server is not None:
            self._server.close()
            await self._server.wait_closed()
            self._server = None

    # -- connection lifecycle ----------------------------------------

    async def _handle(
        self,
        reader: asyncio.StreamReader,
        writer: asyncio.StreamWriter,
    ) -> None:
        client = Client(reader, writer)
        self._clients[client.id] = client
        try:
            await protocol.write_frame(
                writer,
                {
                    "type": protocol.WELCOME,
                    "payload": protocol.welcome_payload(
                        _raydriver_version(),
                        self.config.token is not None,
                    ),
                },
            )
            if not await self._authenticate(client):
                return
            self._spawn(client, self._writer_loop(client))
            self._spawn(client, self._keepalive_loop(client))
            await self._request_loop(client)
        except (ConnectionError, asyncio.IncompleteReadError):
            pass
        except protocol.ProtocolError as err:
            logger.warning("client %s dropped: %s", client.id, err)
        except OverflowError:
            logger.warning("client %s dropped: send queue overflow", client.id)
        finally:
            await self._disconnect(client)

    async def _authenticate(self, client: Client) -> bool:
        token = self.config.token
        if token is None:
            client.authenticated = True
            return True
        try:
            msg = await asyncio.wait_for(
                protocol.read_frame(client.reader), AUTH_TIMEOUT
            )
        except (asyncio.TimeoutError, ConnectionError):
            return False
        client.last_rx = time.monotonic()
        msg_id = msg.get("id", 0)
        supplied = str(msg.get("payload", {}).get("token", ""))
        if msg.get("type") != protocol.AUTH or not hmac.compare_digest(
            supplied, token
        ):
            await protocol.write_frame(
                client.writer,
                protocol.error_response(
                    msg_id, protocol.AUTH_FAILED, "invalid token"
                ),
            )
            return False
        client.authenticated = True
        await protocol.write_frame(client.writer, protocol.response(msg_id))
        return True

    async def _request_loop(self, client: Client) -> None:
        while True:
            msg = await protocol.read_frame(client.reader)
            client.last_rx = time.monotonic()
            reply = await self._dispatch(client, msg)
            if reply is not None:
                client.send(reply)

    async def _writer_loop(self, client: Client) -> None:
        try:
            while True:
                frame = await client.queue.get()
                await protocol.write_frame(client.writer, frame)
        except (ConnectionError, OSError):
            await self._disconnect(client)

    async def _keepalive_loop(self, client: Client) -> None:
        cfg = self.config
        deadline = cfg.keepalive_interval * cfg.keepalive_misses
        while True:
            await asyncio.sleep(cfg.keepalive_interval)
            if time.monotonic() - client.last_rx > deadline:
                await self._disconnect(client)
                return
            try:
                client.send({"type": protocol.PING})
            except OverflowError:
                await self._disconnect(client)
                return

    async def _disconnect(self, client: Client) -> None:
        if client.closed:
            return
        client.closed = True
        for host in list(self._hosts.values()):
            if client.id in host.subscribers:
                self._unsubscribe(host, client)
        for task in client.tasks:
            task.cancel()
        self._clients.pop(client.id, None)
        client.writer.close()
        try:
            await client.writer.wait_closed()
        except (ConnectionError, OSError):
            pass

    def _spawn(self, client: Client, coro: Any) -> None:
        client.tasks.append(asyncio.ensure_future(coro))

    # -- dispatch ------------------------------------------------------

    async def _dispatch(
        self, client: Client, msg: dict[str, Any]
    ) -> dict[str, Any] | None:
        msg_id = msg.get("id", 0)
        type_ = msg.get("type")
        payload = msg.get("payload") or {}
        if type_ == protocol.PING:
            return {"type": protocol.PONG}
        if type_ == protocol.PONG:
            return None
        if type_ == protocol.AUTH:
            return protocol.response(msg_id)
        handler = self._handlers.get(type_ or "")
        if handler is None:
            return protocol.error_response(
                msg_id,
                protocol.UNKNOWN_TYPE,
                f"unknown type {type_!r}",
            )
        try:
            result = await handler(client, payload)
            return protocol.response(msg_id, result)
        except protocol.ProtocolError as err:
            return protocol.error_response(msg_id, err.code, err.message)
        except Exception as err:
            logger.exception("daemon command failed")
            return protocol.error_response(msg_id, protocol.INTERNAL, str(err))

    async def _device_list(
        self, client: Client, payload: dict[str, Any]
    ) -> dict[str, Any]:
        return {
            "ports": list_serial_ports(),
            "cameras": self.config.cameras,
        }

    async def _session_list(
        self, client: Client, payload: dict[str, Any]
    ) -> dict[str, Any]:
        return {
            "sessions": [
                {
                    "session_id": host.id,
                    "protocol": host.protocol_name,
                    "job": host.job,
                    "lease_holder": host.lease_holder,
                    "watchers": len(host.subscribers),
                }
                for host in self._hosts.values()
            ]
        }

    async def _session_open(
        self, client: Client, payload: dict[str, Any]
    ) -> dict[str, Any]:
        host = self._open_host(
            payload.get("protocol", ""),
            payload.get("config") or {},
            payload.get("dialect") or {},
        )
        self._subscribe(host, client, payload)
        return {
            "session_id": host.id,
            "protocol": host.protocol_name,
            "role": client.roles[host.id],
            "lease_holder": host.lease_holder,
        }

    async def _session_attach(
        self, client: Client, payload: dict[str, Any]
    ) -> dict[str, Any]:
        host = self._host(payload.get("session_id"))
        self._subscribe(host, client, payload)
        after_seq = payload.get("after_seq")
        if (
            after_seq is not None
            and host.ring
            and host.ring[0][0] <= after_seq + 1
        ):
            replayed = 0
            for seq, event in host.ring:
                if seq > after_seq:
                    client.send(protocol.event_frame(host.id, seq, event))
                    replayed += 1
            return {"replayed": replayed, "next_seq": host.next_seq}
        return {"snapshot": self._snapshot(host)}

    async def _session_detach(
        self, client: Client, payload: dict[str, Any]
    ) -> dict[str, Any]:
        host = self._host(payload.get("session_id"))
        self._unsubscribe(host, client)
        return {}

    async def _session_close(
        self, client: Client, payload: dict[str, Any]
    ) -> dict[str, Any]:
        host = self._host(payload.get("session_id"))
        self._require_control(host, client)
        await self._close_host(host)
        return {}

    async def _session_command(
        self, client: Client, payload: dict[str, Any]
    ) -> dict[str, Any]:
        host = self._host(payload.get("session_id"))
        self._require_control(host, client)
        method = payload.get("method", "")
        args = payload.get("args") or {}
        if method not in protocol.SESSION_METHODS or not hasattr(
            host.session, method
        ):
            raise protocol.ProtocolError(
                protocol.UNKNOWN_METHOD, f"unknown method {method!r}"
            )
        if method == "run":
            return self._start_job(host, args)
        result = getattr(host.session, method)(**args)
        if inspect.isawaitable(result):
            result = await result
        return {"result": result}

    async def _session_property(
        self, client: Client, payload: dict[str, Any]
    ) -> dict[str, Any]:
        host = self._host(payload.get("session_id"))
        name = payload.get("name", "")
        if name not in protocol.SESSION_PROPERTIES or not hasattr(
            host.session, name
        ):
            raise protocol.ProtocolError(
                protocol.UNKNOWN_PROPERTY,
                f"unknown property {name!r}",
            )
        value = getattr(host.session, name)
        if name == "state":
            value = protocol.encode_state(value)
        return {"value": value}

    # -- sessions ------------------------------------------------------

    def _open_host(
        self,
        protocol_name: str,
        config: dict[str, Any],
        dialect: dict[str, Any],
    ) -> SessionHost:
        if protocol_name not in SESSION_CLASSES:
            raise protocol.ProtocolError(
                protocol.BAD_REQUEST,
                f"unknown protocol {protocol_name!r}",
            )
        host = SessionHost(
            uuid.uuid4().hex[:12],
            protocol_name,
            config,
            dialect,
            self.config.ring_size,
        )
        host.session = self._factory(
            protocol_name,
            config,
            dialect,
            lambda event: self._on_session_event(host, event),
        )
        self._hosts[host.id] = host
        return host

    async def _close_host(self, host: SessionHost) -> None:
        self._cancel_grace(host)
        if host.job_task is not None:
            host.job_task.cancel()
        try:
            await host.session.disconnect()
        except Exception:
            logger.exception("session disconnect failed")
        self._fan_out(
            host,
            {
                "kind": protocol.KIND_LEASE,
                "holder": None,
                "reason": "closed",
            },
        )
        for client in list(host.subscribers.values()):
            client.roles.pop(host.id, None)
        host.subscribers.clear()
        self._hosts.pop(host.id, None)

    def _host(self, session_id: Any) -> SessionHost:
        host = self._hosts.get(session_id or "")
        if host is None:
            raise protocol.ProtocolError(
                protocol.UNKNOWN_SESSION,
                f"unknown session {session_id!r}",
            )
        return host

    def _snapshot(self, host: SessionHost) -> dict[str, Any]:
        return {
            "config": host.config,
            "dialect": host.dialect,
            "state": protocol.encode_state(host.session.state),
            "connection_status": host.connection_status,
            "job": host.job,
            "next_seq": host.next_seq,
        }

    # -- subscriptions and lease ---------------------------------------

    def _subscribe(
        self,
        host: SessionHost,
        client: Client,
        payload: dict[str, Any],
    ) -> None:
        role = payload.get("role", "watch")
        if role == "control":
            self._request_lease(host, client, force=bool(payload.get("force")))
        else:
            client.roles[host.id] = "watch"
        host.subscribers[client.id] = client

    def _unsubscribe(self, host: SessionHost, client: Client) -> None:
        host.subscribers.pop(client.id, None)
        client.roles.pop(host.id, None)
        if host.lease_holder == client.id:
            self._start_grace(host, client.id)

    def _require_control(self, host: SessionHost, client: Client) -> None:
        if client.roles.get(host.id) != "control":
            raise protocol.ProtocolError(
                protocol.LEASE_HELD,
                "command requires the control lease",
            )

    def _request_lease(
        self, host: SessionHost, client: Client, force: bool = False
    ) -> None:
        holder = host.lease_holder
        if holder is None or holder == client.id:
            self._grant_lease(host, client)
            return
        if host.lease_in_grace:
            # A reconnect during the grace period keeps control.
            self._grant_lease(host, client)
            return
        if force:
            old = self._clients.get(holder)
            if old is not None:
                old.roles[host.id] = "watch"
            self._grant_lease(host, client, reason="stolen")
            return
        raise protocol.ProtocolError(
            protocol.LEASE_TAKEN,
            f"control lease held by {holder}",
        )

    def _grant_lease(
        self,
        host: SessionHost,
        client: Client,
        reason: str = "granted",
    ) -> None:
        self._cancel_grace(host)
        host.lease_holder = client.id
        client.roles[host.id] = "control"
        self._fan_out(
            host,
            {
                "kind": protocol.KIND_LEASE,
                "holder": client.id,
                "reason": reason,
            },
        )

    def _start_grace(self, host: SessionHost, client_id: str) -> None:
        if host.lease_holder != client_id or host.lease_in_grace:
            return
        host.lease_in_grace = True
        host.grace_task = asyncio.ensure_future(
            self._grace_release(host, client_id)
        )

    async def _grace_release(self, host: SessionHost, client_id: str) -> None:
        await asyncio.sleep(self.config.lease_grace)
        if host.lease_holder == client_id and host.lease_in_grace:
            host.lease_holder = None
            host.lease_in_grace = False
            self._fan_out(
                host,
                {
                    "kind": protocol.KIND_LEASE,
                    "holder": None,
                    "reason": "released",
                },
            )

    def _cancel_grace(self, host: SessionHost) -> None:
        if host.grace_task is not None:
            host.grace_task.cancel()
            host.grace_task = None
        host.lease_in_grace = False

    # -- events and jobs -------------------------------------------------

    def _fan_out(self, host: SessionHost, event: dict[str, Any]) -> None:
        seq = host.next_seq
        host.next_seq += 1
        host.ring.append((seq, event))
        frame = protocol.event_frame(host.id, seq, event)
        for client in list(host.subscribers.values()):
            try:
                client.send(frame)
            except OverflowError:
                host.subscribers.pop(client.id, None)
                asyncio.ensure_future(self._disconnect(client))

    def _on_session_event(self, host: SessionHost, event: Any) -> None:
        if isinstance(event, SessionEvent.ConnectionStatusChanged):
            host.connection_status = event.status
        if (
            isinstance(event, SessionEvent.JobFinished)
            and host.job is not None
        ):
            host.job["running"] = False
        self._fan_out(host, protocol.encode_event(event))

    def _on_progress(self, host: SessionHost, op_index: int) -> None:
        if host.job is not None:
            host.job["current_op"] = op_index
        self._fan_out(host, {"kind": protocol.KIND_PROGRESS, "op": op_index})

    def _start_job(
        self, host: SessionHost, args: dict[str, Any]
    ) -> dict[str, Any]:
        if host.job is not None and host.job["running"]:
            raise protocol.ProtocolError(
                protocol.JOB_RUNNING, "a job is already running"
            )
        op_pairs = args.get("op_line_map") or []
        op_map = {int(line): int(op) for line, op in op_pairs}
        handle = uuid.uuid4().hex[:12]
        host.job = {
            "handle": handle,
            "total_ops": max(op_map.values()) + 1 if op_map else None,
            "current_op": 0,
            "running": True,
            "submitted_at": time.time(),
        }
        kwargs: dict[str, Any] = {
            "op_line_map": op_map or None,
            "progress_callback": lambda op: self._on_progress(host, op),
        }
        if args.get("op_estimates") is not None:
            kwargs["op_estimates"] = args["op_estimates"]
        coro = host.session.run(args.get("gcode", ""), **kwargs)
        host.job_task = asyncio.ensure_future(self._job_wrapper(host, coro))
        return {"job": handle}

    async def _job_wrapper(self, host: SessionHost, coro: Any) -> None:
        try:
            await coro
        except asyncio.CancelledError:
            raise
        except Exception as err:
            if host.job is not None:
                host.job["running"] = False
            self._fan_out(
                host,
                {"kind": protocol.KIND_JOB_ERROR, "message": str(err)},
            )
        else:
            if host.job is not None:
                host.job["running"] = False
