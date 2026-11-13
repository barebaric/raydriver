"""Optional mDNS announce for the daemon (``_raydriver._tcp``).

Requires the ``raydriver[daemon]`` extra (python-zeroconf). When
the extra is missing, :func:`announce` logs a warning and returns
``None`` — mDNS is opt-in and never a hard dependency. Discovery
consumers stay in Rayforge, which already runs an mDNS channel.
"""

from __future__ import annotations

import logging
import socket
from typing import Any

try:
    from zeroconf import (  # pyright: ignore[reportMissingImports]
        ServiceInfo,
        Zeroconf,
    )
except ImportError:  # optional extra: raydriver[daemon]
    ServiceInfo = None  # type: ignore[assignment,misc]
    Zeroconf = None  # type: ignore[assignment,misc]

logger = logging.getLogger(__name__)

SERVICE_TYPE = "_raydriver._tcp.local."


def _local_ip() -> str:
    try:
        return socket.gethostbyname(socket.gethostname())
    except OSError:
        return "127.0.0.1"


class Announcer:
    """A registered mDNS service; close() unregisters it."""

    def __init__(self, zeroconf: Any, info: Any) -> None:
        self._zeroconf = zeroconf
        self.info = info

    async def close(self) -> None:
        await self._zeroconf.async_unregister_service(self.info)
        self._zeroconf.close()


async def announce(config: Any, port: int) -> Announcer | None:
    """Announce the daemon on the LAN; None without the extra."""
    if Zeroconf is None or ServiceInfo is None:
        logger.warning(
            "mdns enabled but python-zeroconf is not installed "
            "(pip install raydriver[daemon])"
        )
        return None
    zeroconf = Zeroconf()
    name = f"raydriver-{socket.gethostname()}.{SERVICE_TYPE}"
    info = ServiceInfo(
        SERVICE_TYPE,
        name,
        addresses=[socket.inet_aton(_local_ip())],
        port=port,
        properties={
            "protocol": "1",
            "requires_auth": str(config.token is not None).lower(),
            "tls": str(config.tls_cert is not None).lower(),
            "cameras": str(len(config.cameras)),
        },
    )
    await zeroconf.async_register_service(info)
    return Announcer(zeroconf, info)
