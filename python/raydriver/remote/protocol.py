"""Wire protocol for remote sessions (v1).

Length-prefixed JSON frames over TCP (optionally TLS): a 4-byte
big-endian payload length followed by one UTF-8 JSON object. The
envelope is a stable, versioned wire format, so third parties can
drive daemons without this library.

This module is pure codec: framing, envelope helpers, the message
catalog and the event/state codecs. The daemon and the client proxy
build on it; neither the framing nor the envelope knows anything
about a particular firmware protocol.
"""

from __future__ import annotations

import asyncio
import json
import struct
from typing import Any

from raydriver.core import SessionEvent
from raydriver.core.types import DeviceError, DeviceState, DeviceStatus

#: The only protocol version this library speaks. v1 is exact-match
#: until a v2 exists.
PROTOCOL_VERSION = 1

#: One frame = 4-byte big-endian length + UTF-8 JSON payload.
LENGTH_PREFIX = struct.Struct(">I")

#: Job submission is a single frame; chunked upload is a v2 concern.
MAX_FRAME_BYTES = 16 * 1024 * 1024

#: Idle keepalive defaults: a dead peer is detected within
#: interval * misses (~6 s). Both are configurable on the daemon
#: and the client.
DEFAULT_KEEPALIVE_INTERVAL = 2.0
DEFAULT_KEEPALIVE_MISSES = 3

# Client → daemon request types.
AUTH = "auth"
PING = "ping"
PONG = "pong"
DEVICE_LIST = "device/list"
SESSION_LIST = "session/list"
SESSION_OPEN = "session/open"
SESSION_ATTACH = "session/attach"
SESSION_DETACH = "session/detach"
SESSION_CLOSE = "session/close"
SESSION_COMMAND = "session/command"
SESSION_PROPERTY = "session/property"
CLIENT_LIST = "client/list"

# Daemon → client pushes (no correlation id).
WELCOME = "welcome"
EVENT = "event"

#: Every type a client may send. ``welcome`` and ``event`` are
#: daemon-only; a request carrying them is an ``unknown_type``.
REQUEST_TYPES = frozenset(
    {
        AUTH,
        PING,
        PONG,
        DEVICE_LIST,
        SESSION_LIST,
        SESSION_OPEN,
        SESSION_ATTACH,
        SESSION_DETACH,
        SESSION_CLOSE,
        SESSION_COMMAND,
        SESSION_PROPERTY,
        CLIENT_LIST,
    }
)

# Error codes carried by error frames.
BAD_REQUEST = "bad_request"
BAD_VERSION = "bad_version"
AUTH_FAILED = "auth_failed"
UNKNOWN_TYPE = "unknown_type"
UNKNOWN_SESSION = "unknown_session"
UNKNOWN_METHOD = "unknown_method"
UNKNOWN_PROPERTY = "unknown_property"
LEASE_TAKEN = "lease_taken"
LEASE_HELD = "lease_held"
JOB_RUNNING = "job_running"
FRAME_TOO_LARGE = "frame_too_large"
INTERNAL = "internal"

#: Session protocols the daemon can host.
PROTOCOLS = ("grbl", "grbl_simple", "smoothie", "marlin")

#: The full session command surface. ``clear_alarm`` exists on
#: Smoothie only; the daemon rejects methods a session does not
#: implement.
SESSION_METHODS = frozenset(
    {
        "connect",
        "disconnect",
        "run",
        "run_raw",
        "cancel",
        "set_hold",
        "execute_command",
        "execute_interactive_command",
        "home",
        "move_to",
        "jog",
        "select_tool",
        "set_power",
        "set_focus_power",
        "read_settings",
        "write_setting",
        "set_wcs_offset",
        "read_wcs_offsets",
        "read_parser_state",
        "run_probe_cycle",
        "detect_unit_system",
        "update_dialect",
        "clear_alarm",
    }
)

#: Readable session properties. The last three are full-GRBL only.
SESSION_PROPERTIES = frozenset(
    {
        "state",
        "job_running",
        "resource_uri",
        "buffer_count",
        "rx_buffer_size",
        "pending_commands",
    }
)

#: Daemon-synthesized event kinds (not SessionEvent variants):
#: progress bridges the Rust progress callback, job_error reports a
#: failed job, lease reports control-lease changes.
KIND_PROGRESS = "progress"
KIND_JOB_ERROR = "job_error"
KIND_LEASE = "lease"


class ProtocolError(Exception):
    """A wire-level error, surfaced to the peer as an error frame."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message

    def payload(self) -> dict[str, str]:
        """The ``error`` object of an error frame."""
        return {"code": self.code, "message": self.message}


def encode_frame(message: dict[str, Any]) -> bytes:
    """Serialize *message* as one length-prefixed JSON frame."""
    payload = json.dumps(
        message, ensure_ascii=False, separators=(",", ":")
    ).encode("utf-8")
    if len(payload) > MAX_FRAME_BYTES:
        raise ProtocolError(
            FRAME_TOO_LARGE,
            f"frame of {len(payload)} bytes exceeds the "
            f"{MAX_FRAME_BYTES}-byte limit",
        )
    return LENGTH_PREFIX.pack(len(payload)) + payload


async def read_frame(reader: asyncio.StreamReader) -> dict[str, Any]:
    """Read one frame, raising on truncation, oversize or bad JSON.

    A closed connection (clean or mid-frame) raises
    :class:`ConnectionError`; wire violations raise
    :class:`ProtocolError`.
    """
    try:
        prefix = await reader.readexactly(LENGTH_PREFIX.size)
    except asyncio.IncompleteReadError as err:
        raise ConnectionError("connection closed") from err
    (size,) = LENGTH_PREFIX.unpack(prefix)
    if size > MAX_FRAME_BYTES:
        raise ProtocolError(
            FRAME_TOO_LARGE,
            f"frame of {size} bytes exceeds the {MAX_FRAME_BYTES}-byte limit",
        )
    try:
        payload = await reader.readexactly(size)
    except asyncio.IncompleteReadError as err:
        raise ConnectionError("connection closed mid-frame") from err
    try:
        message = json.loads(payload.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as err:
        raise ProtocolError(
            BAD_REQUEST, "frame is not valid UTF-8 JSON"
        ) from err
    if not isinstance(message, dict):
        raise ProtocolError(BAD_REQUEST, "frame must be a JSON object")
    return message


async def write_frame(
    writer: asyncio.StreamWriter, message: dict[str, Any]
) -> None:
    """Write one frame and drain the transport."""
    writer.write(encode_frame(message))
    await writer.drain()


def request(
    msg_id: int, type_: str, payload: dict[str, Any] | None = None
) -> dict[str, Any]:
    """A client → daemon request envelope."""
    return {"id": msg_id, "type": type_, "payload": payload or {}}


def response(
    msg_id: int, payload: dict[str, Any] | None = None
) -> dict[str, Any]:
    """A success response echoing the request's correlation id."""
    return {"id": msg_id, "ok": True, "payload": payload or {}}


def error_response(msg_id: int, code: str, message: str) -> dict[str, Any]:
    """An error response echoing the request's correlation id."""
    return {
        "id": msg_id,
        "ok": False,
        "error": {"code": code, "message": message},
    }


def event_frame(
    session_id: str, seq: int, event: dict[str, Any]
) -> dict[str, Any]:
    """A daemon → client event push (no correlation id)."""
    return {
        "type": EVENT,
        "payload": {
            "session": session_id,
            "seq": seq,
            "event": event,
        },
    }


def welcome_payload(
    raydriver_version: str, auth_required: bool
) -> dict[str, Any]:
    """The ``welcome`` push sent before anything else."""
    return {
        "protocol": PROTOCOL_VERSION,
        "raydriver": raydriver_version,
        "auth_required": auth_required,
    }


def encode_state(state: DeviceState) -> dict[str, Any]:
    """The wire form of a device state (status, error, positions)."""
    error = state.error
    return {
        "status": state.status.name,
        "error": (
            None
            if error is None
            else {
                "code": error.code,
                "title": error.title,
                "description": error.description,
            }
        ),
        "machine_pos": list(state.machine_pos),
        "work_pos": list(state.work_pos),
        "wco": list(state.wco),
        "feed_rate": state.feed_rate,
        "spindle_speed": state.spindle_speed,
        "buffer_available": state.buffer_available,
        "buffer_rx_available": state.buffer_rx_available,
    }


def decode_state(payload: dict[str, Any]) -> DeviceState:
    """Rebuild a :class:`DeviceState` from its wire form."""
    state = DeviceState()
    status = getattr(DeviceStatus, payload["status"], None)
    if not isinstance(status, DeviceStatus):
        raise ProtocolError(
            BAD_REQUEST, f"unknown device status {payload['status']!r}"
        )
    state.status = status
    error = payload.get("error")
    if error is not None:
        state.error = DeviceError(
            error["code"], error["title"], error["description"]
        )
    state.machine_pos = payload.get("machine_pos", [])
    state.work_pos = payload.get("work_pos", [])
    state.wco = payload.get("wco", [])
    state.feed_rate = payload.get("feed_rate")
    state.spindle_speed = payload.get("spindle_speed")
    state.buffer_available = payload.get("buffer_available")
    state.buffer_rx_available = payload.get("buffer_rx_available")
    return state


def encode_event(event: SessionEvent) -> dict[str, Any]:
    """The wire form of a session event.

    Only the nine :class:`SessionEvent` variants cross the wire as
    events; the daemon-synthesized kinds (progress, job_error,
    lease) are built by the daemon directly.
    """
    match event:
        case SessionEvent.StateChanged():
            return {"kind": "state", "state": encode_state(event.state)}
        case SessionEvent.ConnectionStatusChanged():
            return {
                "kind": "connection_status",
                "status": event.status,
                "message": event.message,
            }
        case SessionEvent.CommandStatusChanged():
            return {
                "kind": "command_status",
                "status": event.status,
                "message": event.message,
            }
        case SessionEvent.JobFinished():
            return {"kind": "job_finished"}
        case SessionEvent.ProbeStatusChanged():
            return {"kind": "probe_status", "message": event.message}
        case SessionEvent.WcsUpdated():
            return {
                "kind": "wcs_updated",
                "offsets": {
                    key: list(value) for key, value in event.offsets.items()
                },
            }
        case SessionEvent.ConfigChanged():
            return {
                "kind": "config_changed",
                "key": event.key,
                "value": event.value,
            }
        case SessionEvent.RawRx():
            return {"kind": "raw_rx", "line": event.line}
        case SessionEvent.RawTx():
            return {"kind": "raw_tx", "line": event.line}
        case _:
            raise ProtocolError(BAD_REQUEST, f"cannot encode event {event!r}")


def decode_event(payload: dict[str, Any]) -> SessionEvent | None:
    """Rebuild a session event from its wire form.

    Returns ``None`` for unknown kinds: clients must ignore event
    kinds they do not know (forward compatibility). The
    daemon-synthesized kinds are handled by the client before
    decoding. Malformed payloads raise :class:`KeyError` or
    :class:`ProtocolError`; the caller decides how to surface them.
    """
    match payload.get("kind"):
        case "state":
            return SessionEvent.StateChanged(decode_state(payload["state"]))
        case "connection_status":
            return SessionEvent.ConnectionStatusChanged(
                payload["status"], payload.get("message")
            )
        case "command_status":
            return SessionEvent.CommandStatusChanged(
                payload["status"], payload.get("message")
            )
        case "job_finished":
            return SessionEvent.JobFinished()
        case "probe_status":
            return SessionEvent.ProbeStatusChanged(payload["message"])
        case "wcs_updated":
            return SessionEvent.WcsUpdated(
                {
                    key: tuple(value)
                    for key, value in payload["offsets"].items()
                }
            )
        case "config_changed":
            return SessionEvent.ConfigChanged(payload["key"], payload["value"])
        case "raw_rx":
            return SessionEvent.RawRx(payload["line"])
        case "raw_tx":
            return SessionEvent.RawTx(payload["line"])
    return None
