"""Client-side remote session proxy and session factory.

``create_session`` returns a local Rust session when the config
has no remote keys, and a :class:`RemoteSession` proxy otherwise.
The proxy forwards commands and re-emits events through the same
``event_callback`` surface the shells already wire, so a driver
shell works locally or remotely with a one-line construction
change. Video never crosses this link; cameras are ordinary
network streams (see the remote-driver design).
"""

from __future__ import annotations

import asyncio
import logging
from typing import Any, Callable
from urllib.parse import urlsplit

from raydriver.analyzer.listener import make_client_ssl_context
from raydriver.core import SessionEvent
from raydriver.core.types import DeviceState
from raydriver.remote import protocol
from raydriver.remote.daemon import DEFAULT_PORT, default_session_factory

logger = logging.getLogger(__name__)

#: Config keys that select and describe the remote endpoint; they
#: are stripped from the config forwarded to the daemon.
REMOTE_KEYS = (
    "remote_host",
    "remote_port",
    "token",
    "tls_ca",
    "tls_hostname",
    "url",
)


def _remote_endpoint(config: dict[str, Any]) -> dict[str, Any] | None:
    """Extract the endpoint settings, or ``None`` for local use."""
    url = config.get("url")
    if url:
        parts = urlsplit(url)
        if parts.scheme != "raydriver":
            raise ValueError(f"unsupported remote url {url!r}")
        if not parts.hostname:
            raise ValueError(f"remote url has no host: {url!r}")
        return {
            "remote_host": parts.hostname,
            "remote_port": parts.port or DEFAULT_PORT,
            "token": parts.fragment or None,
            "tls_ca": config.get("tls_ca"),
            "tls_hostname": config.get("tls_hostname"),
        }
    if config.get("remote_host"):
        return {
            "remote_host": config["remote_host"],
            "remote_port": config.get("remote_port") or DEFAULT_PORT,
            "token": config.get("token"),
            "tls_ca": config.get("tls_ca"),
            "tls_hostname": config.get("tls_hostname"),
        }
    return None


def create_session(
    protocol_name: str,
    config: dict[str, Any] | None,
    dialect: dict[str, Any] | None,
    event_callback: Callable[[Any], None] | None,
) -> Any:
    """Route to a local Rust session or a remote proxy.

    Remote is a configuration choice: ``remote_host`` +
    ``remote_port`` + ``token`` + ``tls_ca``/``tls_hostname``, or
    a single ``url`` (``raydriver://host:port#token``). Plain
    ``host``/``port`` keep their local meaning.
    """
    config = config or {}
    endpoint = _remote_endpoint(config)
    if endpoint is None:
        return default_session_factory(
            protocol_name, config, dialect, event_callback
        )
    local_config = {
        key: value for key, value in config.items() if key not in REMOTE_KEYS
    }
    return RemoteSession(
        protocol_name, local_config, dialect, event_callback, endpoint
    )


class RemoteSession:
    """Proxy with the union of the sessions' method surfaces.

    Commands become ``session/command`` frames awaiting the
    correlated response; events from the reader task re-emerge
    through the constructor-supplied ``event_callback`` on the
    event loop. Properties are cached from events, snapshots and
    property reads; ``resource_uri`` is fetched on connect.
    """

    def __init__(
        self,
        protocol_name: str,
        config: dict[str, Any],
        dialect: dict[str, Any] | None,
        event_callback: Callable[[Any], None] | None,
        endpoint: dict[str, Any],
    ) -> None:
        self._protocol_name = protocol_name
        self._config = config
        self._dialect = dialect
        self._event_callback = event_callback
        self._endpoint = endpoint
        self._reader: asyncio.StreamReader | None = None
        self._writer: asyncio.StreamWriter | None = None
        self._reader_task: asyncio.Task | None = None
        self._writer_task: asyncio.Task | None = None
        self._outgoing: asyncio.Queue | None = None
        self._session_id: str | None = None
        self._next_id = 0
        self._pending: dict[int, asyncio.Future] = {}
        self._last_seq = 0
        self._connected = False
        self._closing = False
        self._lock = asyncio.Lock()
        self._state = DeviceState()
        self._job_running = False
        self._resource_uri: str | None = None
        self._buffer_count = 0
        self._rx_buffer_size = 0
        self._pending_commands: list[str] = []
        self._progress_callback: Callable[[int], None] | None = None
        self._job_done: asyncio.Future | None = None

    # -- cached properties -------------------------------------------

    @property
    def state(self) -> DeviceState:
        return self._state

    @property
    def job_running(self) -> bool:
        return self._job_running

    @property
    def resource_uri(self) -> str | None:
        return self._resource_uri

    @property
    def buffer_count(self) -> int:
        return self._buffer_count

    @property
    def rx_buffer_size(self) -> int:
        return self._rx_buffer_size

    @property
    def pending_commands(self) -> list[str]:
        return self._pending_commands

    # -- connection lifecycle -----------------------------------------

    async def _ensure_connected(self) -> None:
        if self._connected:
            return
        async with self._lock:
            if self._connected:
                return
            await self._open_connection()
            if self._session_id is None:
                reply = await self._request(
                    protocol.SESSION_OPEN,
                    {
                        "protocol": self._protocol_name,
                        "config": self._config,
                        "dialect": self._dialect,
                        "role": "control",
                    },
                )
                self._check(reply)
                self._session_id = reply["payload"]["session_id"]
            else:
                await self._reattach()
            self._connected = True
            await self._refresh_properties()

    async def attach(self) -> None:
        """Reattach after a disconnect, resyncing from last seq."""
        await self._ensure_connected()

    async def _open_connection(self) -> None:
        endpoint = self._endpoint
        host = endpoint["remote_host"]
        port = int(endpoint.get("remote_port") or DEFAULT_PORT)
        token = endpoint.get("token")
        tls_ca = endpoint.get("tls_ca")
        if token and not tls_ca:
            raise ConnectionError("refusing to send a token without TLS")
        ssl_context = make_client_ssl_context(tls_ca) if tls_ca else None
        self._reader, self._writer = await asyncio.open_connection(
            host,
            port,
            ssl=ssl_context,
            server_hostname=(
                (endpoint.get("tls_hostname") or host) if ssl_context else None
            ),
        )
        try:
            welcome = await protocol.read_frame(self._reader)
            if welcome.get("type") != protocol.WELCOME:
                raise ConnectionError("daemon did not send welcome")
            payload = welcome["payload"]
            if payload.get("protocol") != protocol.PROTOCOL_VERSION:
                raise ConnectionError(
                    "unsupported daemon protocol version "
                    f"{payload.get('protocol')!r}"
                )
            if payload.get("auth_required"):
                await protocol.write_frame(
                    self._writer,
                    protocol.request(0, protocol.AUTH, {"token": token or ""}),
                )
                reply = await protocol.read_frame(self._reader)
                if not reply.get("ok"):
                    raise ConnectionError("daemon authentication failed")
        except BaseException:
            self._writer.close()
            raise
        self._outgoing = asyncio.Queue()
        self._closing = False
        self._reader_task = asyncio.ensure_future(self._reader_loop())
        self._writer_task = asyncio.ensure_future(self._writer_loop())

    async def _reattach(self) -> None:
        reply = await self._request(
            protocol.SESSION_ATTACH,
            {
                "session_id": self._session_id,
                "after_seq": self._last_seq,
                "role": "control",
            },
        )
        self._check(reply)
        snapshot = reply["payload"].get("snapshot")
        if snapshot is not None:
            self._apply_snapshot(snapshot)

    def _apply_snapshot(self, snapshot: dict[str, Any]) -> None:
        self._state = protocol.decode_state(snapshot["state"])
        self._last_seq = snapshot["next_seq"] - 1
        job = snapshot.get("job")
        self._job_running = bool(job and job["running"])
        self._emit(SessionEvent.StateChanged(self._state))
        self._emit(
            SessionEvent.ConnectionStatusChanged(
                snapshot["connection_status"], None
            )
        )

    async def _refresh_properties(self) -> None:
        for name in (
            "resource_uri",
            "buffer_count",
            "rx_buffer_size",
            "pending_commands",
        ):
            reply = await self._request(
                protocol.SESSION_PROPERTY,
                {"session_id": self._session_id, "name": name},
            )
            if reply.get("ok"):
                setattr(self, f"_{name}", reply["payload"]["value"])

    async def aclose(self) -> None:
        """Detach from the session and close the connection."""
        self._closing = True
        if self._connected and self._session_id is not None:
            try:
                await self._request(
                    protocol.SESSION_DETACH,
                    {"session_id": self._session_id},
                )
            except ConnectionError:
                pass
        await self._teardown()

    async def _teardown(self) -> None:
        self._connected = False
        for task in (self._reader_task, self._writer_task):
            if task is not None:
                task.cancel()
        self._reader_task = None
        self._writer_task = None
        if self._writer is not None:
            self._writer.close()
            try:
                await self._writer.wait_closed()
            except (ConnectionError, OSError):
                pass
        self._writer = None
        self._reader = None

    # -- wire plumbing --------------------------------------------------

    def _enqueue(self, frame: dict[str, Any]) -> None:
        assert self._outgoing is not None
        self._outgoing.put_nowait(frame)

    async def _request(
        self, type_: str, payload: dict[str, Any]
    ) -> dict[str, Any]:
        self._next_id += 1
        msg_id = self._next_id
        future = asyncio.get_running_loop().create_future()
        self._pending[msg_id] = future
        self._enqueue(protocol.request(msg_id, type_, payload))
        return await future

    def _check(self, reply: dict[str, Any]) -> None:
        if not reply.get("ok"):
            error = reply.get("error", {})
            raise ConnectionError(
                error.get("message", "remote command failed")
            )

    async def _writer_loop(self) -> None:
        assert self._outgoing is not None
        try:
            while True:
                frame = await self._outgoing.get()
                assert self._writer is not None
                await protocol.write_frame(self._writer, frame)
        except (ConnectionError, OSError):
            self._on_disconnect("connection lost")

    async def _reader_loop(self) -> None:
        assert self._reader is not None
        try:
            while True:
                msg = await protocol.read_frame(self._reader)
                type_ = msg.get("type")
                if type_ == protocol.PING:
                    self._enqueue({"type": protocol.PONG})
                elif type_ == protocol.EVENT:
                    self._handle_event(msg["payload"])
                elif type_ != protocol.PONG:
                    msg_id = msg.get("id")
                    future = (
                        self._pending.pop(msg_id, None)
                        if isinstance(msg_id, int)
                        else None
                    )
                    if future is not None and not future.done():
                        future.set_result(msg)
        except (
            ConnectionError,
            asyncio.IncompleteReadError,
            OSError,
            protocol.ProtocolError,
        ):
            self._on_disconnect("connection lost")

    def _handle_event(self, payload: dict[str, Any]) -> None:
        self._last_seq = max(self._last_seq, payload.get("seq", 0))
        event = payload.get("event", {})
        kind = event.get("kind")
        if kind == protocol.KIND_PROGRESS:
            if self._progress_callback is not None:
                self._progress_callback(event.get("op", 0))
            return
        if kind == protocol.KIND_JOB_ERROR:
            self._job_running = False
            self._finish_job()
            return
        if kind == protocol.KIND_LEASE:
            return
        decoded = protocol.decode_event(event)
        if decoded is None:
            return
        if isinstance(decoded, SessionEvent.StateChanged):
            self._state = decoded.state
        if isinstance(decoded, SessionEvent.JobFinished):
            self._job_running = False
            self._finish_job()
        self._emit(decoded)

    def _emit(self, event: Any) -> None:
        if self._event_callback is not None:
            self._event_callback(event)

    def _finish_job(self) -> None:
        if self._job_done is not None and not self._job_done.done():
            self._job_done.set_result(None)

    def _on_disconnect(self, message: str) -> None:
        if self._closing or not self._connected:
            return
        self._connected = False
        for future in self._pending.values():
            if not future.done():
                future.set_exception(ConnectionError(message))
        self._pending.clear()
        if self._job_done is not None and not self._job_done.done():
            self._job_done.set_exception(ConnectionError(message))
        self._emit(SessionEvent.ConnectionStatusChanged("SLEEPING", message))

    # -- command surface -------------------------------------------------

    async def _command(self, method: str, **args: Any) -> Any:
        await self._ensure_connected()
        reply = await self._request(
            protocol.SESSION_COMMAND,
            {
                "session_id": self._session_id,
                "method": method,
                "args": args,
            },
        )
        self._check(reply)
        return reply["payload"].get("result")

    async def connect(self) -> None:
        await self._command("connect")

    async def disconnect(self) -> None:
        await self._command("disconnect")

    async def execute_interactive_command(self, command: str) -> Any:
        return await self._command(
            "execute_interactive_command", command=command
        )

    async def execute_command(self, command: str) -> Any:
        return await self._command("execute_command", command=command)

    async def run(
        self,
        gcode: str,
        op_line_map: Any = None,
        op_estimates: Any = None,
        progress_callback: Callable[[int], None] | None = None,
    ) -> None:
        await self._ensure_connected()
        self._progress_callback = progress_callback
        self._job_done = asyncio.get_running_loop().create_future()
        args: dict[str, Any] = {"gcode": gcode}
        if op_line_map is not None:
            args["op_line_map"] = [
                [int(line), int(op)] for line, op in op_line_map.items()
            ]
        if op_estimates is not None:
            args["op_estimates"] = list(op_estimates)
        reply = await self._request(
            protocol.SESSION_COMMAND,
            {
                "session_id": self._session_id,
                "method": "run",
                "args": args,
            },
        )
        if not reply.get("ok"):
            self._job_done = None
            self._progress_callback = None
            self._check(reply)
        self._job_running = True
        job_done = self._job_done
        assert job_done is not None
        try:
            await job_done
        finally:
            self._job_done = None
            self._progress_callback = None

    async def run_raw(self, machine_code: str) -> None:
        await self._command("run_raw", machine_code=machine_code)

    async def set_hold(self, hold: bool = True) -> None:
        await self._command("set_hold", hold=hold)

    async def cancel(self, emergency: bool = False) -> None:
        await self._command("cancel", emergency=emergency)

    async def home(
        self,
        axes: Any = None,
        active_wcs: str = "G54",
    ) -> None:
        await self._command("home", axes=axes, active_wcs=active_wcs)

    async def move_to(self, speed: float, x: float, y: float) -> None:
        await self._command("move_to", speed=speed, x=x, y=y)

    async def jog(self, speed: float, deltas: Any) -> None:
        await self._command("jog", speed=speed, deltas=deltas)

    async def select_tool(self, tool_number: int) -> None:
        await self._command("select_tool", tool_number=tool_number)

    async def set_power(self, power: float | None = None) -> None:
        await self._command("set_power", power=power)

    async def set_focus_power(self, power: float | None = None) -> None:
        await self._command("set_focus_power", power=power)

    async def read_settings(self) -> Any:
        return await self._command("read_settings")

    async def write_setting(self, key: str, value: str) -> None:
        await self._command("write_setting", key=key, value=value)

    async def set_wcs_offset(
        self,
        wcs_slot: str,
        x: float,
        y: float,
        z: float | None,
    ) -> None:
        await self._command("set_wcs_offset", wcs_slot=wcs_slot, x=x, y=y, z=z)

    async def read_wcs_offsets(self) -> Any:
        return await self._command("read_wcs_offsets")

    async def read_parser_state(self) -> Any:
        return await self._command("read_parser_state")

    async def run_probe_cycle(
        self, axis_letter: str, max_travel: float, feed_rate: float
    ) -> Any:
        return await self._command(
            "run_probe_cycle",
            axis_letter=axis_letter,
            max_travel=max_travel,
            feed_rate=feed_rate,
        )

    async def detect_unit_system(self) -> Any:
        return await self._command("detect_unit_system")

    def update_dialect(self, dialect: dict) -> None:
        # Synchronous like the Rust sessions: enqueue the update so
        # it keeps its order against subsequent commands.
        self._dialect = dialect
        if self._connected:
            self._next_id += 1
            self._enqueue(
                protocol.request(
                    self._next_id,
                    protocol.SESSION_COMMAND,
                    {
                        "session_id": self._session_id,
                        "method": "update_dialect",
                        "args": {"dialect": dialect},
                    },
                )
            )
        else:
            asyncio.ensure_future(self._update_dialect_async(dialect))

    async def _update_dialect_async(self, dialect: dict) -> None:
        try:
            await self._command("update_dialect", dialect=dialect)
        except ConnectionError:
            logger.warning("dialect update deferred: not connected")

    async def clear_alarm(self) -> None:
        await self._command("clear_alarm")
