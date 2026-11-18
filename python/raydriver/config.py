"""Typed session configuration objects.

The Rust sessions accept a plain mapping for the wire protocol, but
application code should use these dataclasses: they spell out the
transport and timing fields for each protocol and reject typos at
type-checking time.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass
from typing import Any


@dataclass(frozen=True, kw_only=True)
class SessionConfig:
    """Base for the typed session configuration dataclasses."""

    def to_dict(self) -> dict[str, Any]:
        """The mapping form expected by the Rust session constructors."""
        return {
            key: value
            for key, value in asdict(self).items()
            if value is not None
        }


@dataclass(frozen=True, kw_only=True)
class GrblSessionOptions(SessionConfig):
    """Timing and flow-control options shared by GRBL sessions."""

    poll_status_while_running: bool = False
    deadlock_detection: bool = False
    rx_buffer_size_override: int = 0
    cached_rx_buffer_size: int | None = None
    stall_timeout_min: float = 5.0
    stall_timeout_max: float = 120.0
    stall_timeout_safety_factor: float = 3.0
    stall_timeout_default: float = 30.0
    safety_shutdown_delay: float = 0.2
    unanswered_poll_limit: int = 3
    poll_response_attempts: int = 10
    poll_response_interval: float = 0.1
    handshake_timeout: float = 6.0
    handshake_poll_interval: float = 0.5
    status_poll_interval: float = 0.5
    reconnect_delay: float = 5.0
    command_timeout: float = 10.0


@dataclass(frozen=True, kw_only=True)
class GrblSerialConfig(GrblSessionOptions):
    port: str
    baudrate: int = 115200


@dataclass(frozen=True, kw_only=True)
class GrblTelnetConfig(GrblSessionOptions):
    host: str
    tcp_port: int = 23


@dataclass(frozen=True, kw_only=True)
class GrblSimpleConfig(GrblSerialConfig):
    """Configuration for the minimal ping-pong GRBL session."""


@dataclass(frozen=True, kw_only=True)
class Esp3dConfig(GrblSessionOptions):
    """ESP3D / FluidNC network session configuration."""

    host: str
    http_port: int = 80
    ws_port: int = 81
    protocol: str = "ESP3D"
    request_timeout: float = 10.0
    connect_timeout: float = 6.0


@dataclass(frozen=True, kw_only=True)
class SmoothieConfig(SessionConfig):
    host: str
    tcp_port: int = 23
    poll_status_while_running: bool = False
    command_timeout: float = 30.0
    handshake_timeout: float = 15.0
    handshake_poll_interval: float = 0.5
    status_poll_interval: float = 1.0
    reconnect_delay: float = 5.0
    safety_shutdown_delay: float = 0.2


@dataclass(frozen=True, kw_only=True)
class MarlinConfig(SessionConfig):
    port: str
    baudrate: int = 115200
    poll_status_while_running: bool = False
    command_timeout: float = 30.0
    handshake_timeout: float = 5.0
    handshake_poll_interval: float = 0.5
    status_poll_interval: float = 2.0
    reconnect_delay: float = 5.0
    connect_settle: float = 1.0


@dataclass(frozen=True, kw_only=True)
class XtoolConfig(SessionConfig):
    host: str
    http_port: int = 8080
    ws_port: int = 8081
    request_timeout: float = 5.0
    connect_timeout: float = 6.0
    reconnect_delay: float = 5.0
    homing_timeout: float = 120.0
    job_start_timeout: float = 15.0
    poll_interval: float = 0.5


@dataclass(frozen=True, kw_only=True)
class M2Config(SessionConfig):
    host: str
    port: int = 28900
    request_timeout: float = 10.0
    connect_timeout: float = 15.0
    reconnect_delay: float = 5.0
    poll_interval: float = 3.0
    tls_allow_invalid_certs: bool = False


@dataclass(frozen=True, kw_only=True)
class RemoteEndpoint(SessionConfig):
    """The daemon endpoint used by remote sessions."""

    remote_host: str
    remote_port: int = 8765
    token: str | None = None
    tls_ca: str | None = None
    tls_hostname: str | None = None


__all__ = [
    "Esp3dConfig",
    "GrblSerialConfig",
    "GrblSessionOptions",
    "GrblSimpleConfig",
    "GrblTelnetConfig",
    "M2Config",
    "MarlinConfig",
    "RemoteEndpoint",
    "SessionConfig",
    "SmoothieConfig",
    "XtoolConfig",
]
