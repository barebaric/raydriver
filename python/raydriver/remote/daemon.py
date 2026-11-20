"""Device-host daemon for remote sessions."""

from typing import TYPE_CHECKING

import raydriver.raydriver as _raydriver  # type: ignore[import-untyped]
from raydriver.esp3d import Esp3dSession
from raydriver.grbl import GrblSession
from raydriver.grbl.simple import GrblSimpleSession
from raydriver.marlin import MarlinSession
from raydriver.smoothie import SmoothieSession

if TYPE_CHECKING:
    from raydriver.remote import ClientQueue, Daemon, DaemonConfig
else:
    ClientQueue = _raydriver.remote.ClientQueue
    Daemon = _raydriver.remote.Daemon
    DaemonConfig = _raydriver.remote.DaemonConfig

DEFAULT_LEASE_GRACE = _raydriver.remote.DEFAULT_LEASE_GRACE
DEFAULT_PORT = _raydriver.remote.DEFAULT_PORT
DEFAULT_RING_SIZE = _raydriver.remote.DEFAULT_RING_SIZE
SEND_QUEUE_LIMIT = _raydriver.remote.SEND_QUEUE_LIMIT

SESSION_CLASSES = {
    "grbl": GrblSession,
    "grbl_simple": GrblSimpleSession,
    "esp3d": Esp3dSession,
    "smoothie": SmoothieSession,
    "marlin": MarlinSession,
}


def default_session_factory(
    protocol_name, config, dialect=None, event_callback=None
):
    cls = SESSION_CLASSES[protocol_name]
    return cls(
        config=config,
        dialect=dialect,
        event_callback=event_callback,
    )


__all__ = [
    "DEFAULT_LEASE_GRACE",
    "DEFAULT_PORT",
    "DEFAULT_RING_SIZE",
    "SEND_QUEUE_LIMIT",
    "ClientQueue",
    "Daemon",
    "DaemonConfig",
    "SESSION_CLASSES",
    "default_session_factory",
]
