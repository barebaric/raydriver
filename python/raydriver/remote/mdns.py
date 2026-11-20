"""mDNS announce and browse for the daemon (``_raydriver._tcp``)."""

import socket
from typing import TYPE_CHECKING

import raydriver.raydriver as _raydriver  # type: ignore[import-untyped]

if TYPE_CHECKING:
    from raydriver.remote import Announcer, ServiceInfo
else:
    Announcer = _raydriver.remote.Announcer
    ServiceInfo = _raydriver.remote.ServiceInfo

SERVICE_TYPE = _raydriver.remote.SERVICE_TYPE
_announce_address = _raydriver.remote._announce_address
_service_name = _raydriver.remote._service_name
_service_name_for = _raydriver.remote._service_name_for
announce = _raydriver.remote.announce
browse_daemons = _raydriver.remote.browse_daemons

__all__ = [
    "SERVICE_TYPE",
    "Announcer",
    "ServiceInfo",
    "_announce_address",
    "_service_name",
    "_service_name_for",
    "announce",
    "browse_daemons",
    "socket",
]
