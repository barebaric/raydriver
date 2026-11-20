"""Wire protocol for remote sessions (v1).

Framing and codecs are implemented in Rust. This module keeps the
exception type and the asyncio stream helpers.
"""

from __future__ import annotations

import struct
from typing import Any

import raydriver.raydriver as _raydriver  # type: ignore[import-untyped]

_protocol = _raydriver.remote

MAX_FRAME_BYTES = _protocol.MAX_FRAME_BYTES
FRAME_TOO_LARGE = _protocol.FRAME_TOO_LARGE
BAD_REQUEST = _protocol.BAD_REQUEST

PROTOCOL_VERSION = _protocol.PROTOCOL_VERSION
DEFAULT_KEEPALIVE_INTERVAL = _protocol.DEFAULT_KEEPALIVE_INTERVAL
DEFAULT_KEEPALIVE_MISSES = _protocol.DEFAULT_KEEPALIVE_MISSES
AUTH = _protocol.AUTH
PING = _protocol.PING
PONG = _protocol.PONG
DEVICE_LIST = _protocol.DEVICE_LIST
PROTOCOL_LIST = _protocol.PROTOCOL_LIST
SESSION_LIST = _protocol.SESSION_LIST
SESSION_OPEN = _protocol.SESSION_OPEN
SESSION_ATTACH = _protocol.SESSION_ATTACH
SESSION_DETACH = _protocol.SESSION_DETACH
SESSION_CLOSE = _protocol.SESSION_CLOSE
SESSION_COMMAND = _protocol.SESSION_COMMAND
SESSION_PROPERTY = _protocol.SESSION_PROPERTY
CLIENT_LIST = _protocol.CLIENT_LIST
WELCOME = _protocol.WELCOME
EVENT = _protocol.EVENT
BAD_VERSION = _protocol.BAD_VERSION
AUTH_FAILED = _protocol.AUTH_FAILED
UNKNOWN_TYPE = _protocol.UNKNOWN_TYPE
UNKNOWN_SESSION = _protocol.UNKNOWN_SESSION
UNKNOWN_DEVICE = _protocol.UNKNOWN_DEVICE
UNKNOWN_METHOD = _protocol.UNKNOWN_METHOD
UNKNOWN_PROPERTY = _protocol.UNKNOWN_PROPERTY
LEASE_TAKEN = _protocol.LEASE_TAKEN
LEASE_HELD = _protocol.LEASE_HELD
JOB_RUNNING = _protocol.JOB_RUNNING
INTERNAL = _protocol.INTERNAL
KIND_PROGRESS = _protocol.KIND_PROGRESS
KIND_JOB_ERROR = _protocol.KIND_JOB_ERROR
KIND_LEASE = _protocol.KIND_LEASE
REQUEST_TYPES = _protocol.REQUEST_TYPES
PROTOCOLS = _protocol.PROTOCOLS
SESSION_METHODS = _protocol.SESSION_METHODS
SESSION_PROPERTIES = _protocol.SESSION_PROPERTIES
LENGTH_PREFIX = struct.Struct(">I")


class ProtocolError(Exception):
    """A wire-level error, surfaced to the peer as an error frame."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message

    def payload(self) -> dict[str, str]:
        return {"code": self.code, "message": self.message}


def encode_frame(message: dict[str, Any]) -> bytes:
    return _protocol.encode_frame(message, MAX_FRAME_BYTES)


def decode_frame(payload: bytes) -> dict[str, Any]:
    return _protocol.decode_frame(payload, MAX_FRAME_BYTES)


async def read_frame(reader) -> dict[str, Any]:
    try:
        prefix = await reader.readexactly(LENGTH_PREFIX.size)
    except Exception as err:
        raise ConnectionError("connection closed") from err
    (size,) = LENGTH_PREFIX.unpack(prefix)
    if size > MAX_FRAME_BYTES:
        raise ProtocolError(
            FRAME_TOO_LARGE,
            f"frame of {size} bytes exceeds the {MAX_FRAME_BYTES}-byte limit",
        )
    try:
        payload = await reader.readexactly(size)
    except Exception as err:
        raise ConnectionError("connection closed mid-frame") from err
    return decode_frame(payload)


async def write_frame(writer, message: dict[str, Any]) -> None:
    writer.write(encode_frame(message))
    await writer.drain()


def __getattr__(name: str):
    return getattr(_protocol, name)
