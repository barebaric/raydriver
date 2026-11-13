"""Optional mDNS announce and browse for the daemon
(``_raydriver._tcp``).

Requires the ``raydriver[daemon]`` extra (python-zeroconf). When
the extra is missing, :func:`announce` logs a warning and returns
``None`` and :func:`browse_daemons` returns an empty list — mDNS is
opt-in and never a hard dependency.
"""

from __future__ import annotations

import asyncio
import logging
import socket
from typing import Any

try:
    from zeroconf import (  # pyright: ignore[reportMissingImports]
        ServiceInfo,
        Zeroconf,
    )
    from zeroconf.asyncio import (  # pyright: ignore[reportMissingImports]
        AsyncServiceBrowser,
        AsyncZeroconf,
    )
except ImportError:  # optional extra: raydriver[daemon]
    ServiceInfo = None  # type: ignore[assignment,misc]
    Zeroconf = None  # type: ignore[assignment,misc]
    AsyncServiceBrowser = None  # type: ignore[assignment,misc]
    AsyncZeroconf = None  # type: ignore[assignment,misc]

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


async def browse_daemons(timeout: float = 3.0) -> list[dict[str, Any]]:
    """Browse for daemons for *timeout* seconds; [] without the
    extra. Each entry has name, host, port and txt fields."""
    if AsyncZeroconf is None or AsyncServiceBrowser is None:
        logger.warning(
            "mdns browse requires python-zeroconf "
            "(pip install raydriver[daemon])"
        )
        return []
    found: set[str] = set()

    class _Listener:
        def add_service(self, zc: Any, type_: str, name: str) -> None:
            found.add(name)

        def remove_service(self, zc: Any, type_: str, name: str) -> None:
            found.discard(name)

        def update_service(self, zc: Any, type_: str, name: str) -> None:
            pass

    results: list[dict[str, Any]] = []
    async with AsyncZeroconf() as azc:
        browser = AsyncServiceBrowser(azc.zeroconf, SERVICE_TYPE, _Listener())
        await asyncio.sleep(timeout)
        await browser.async_cancel()
        for name in sorted(found):
            info = await azc.async_get_service_info(SERVICE_TYPE, name)
            if info is None:
                continue
            addresses = info.parsed_addresses()
            results.append(
                {
                    "name": name,
                    "host": addresses[0] if addresses else "",
                    "port": info.port,
                    "txt": {
                        key.decode(): value.decode()
                        for key, value in info.properties.items()
                    },
                }
            )
    return results
