"""Client-side remote session proxy and session factory."""

from typing import TYPE_CHECKING

import raydriver.raydriver as _raydriver  # type: ignore[import-untyped]

if TYPE_CHECKING:
    from raydriver.remote import RemoteSession
else:
    RemoteSession = _raydriver.remote.RemoteSession

REMOTE_KEYS = _raydriver.remote.REMOTE_KEYS
create_session = _raydriver.remote.create_session
list_devices = _raydriver.remote.list_devices
list_protocols = _raydriver.remote.list_protocols
open_session = _raydriver.remote.open_session

__all__ = [
    "REMOTE_KEYS",
    "RemoteSession",
    "create_session",
    "list_devices",
    "list_protocols",
    "open_session",
]
