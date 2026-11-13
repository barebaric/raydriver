"""mDNS announce and browse for the daemon (``_raydriver._tcp``).

Uses zeroconf, a default dependency, so discovery works out of the
box with a plain ``pip install raydriver``.
"""

from __future__ import annotations

import asyncio
import logging
import socket
from typing import Any

from zeroconf import ServiceInfo, ServiceListener, Zeroconf
from zeroconf.asyncio import AsyncServiceBrowser, AsyncZeroconf

from raydriver.analyzer.listener import is_loopback

logger = logging.getLogger(__name__)

SERVICE_TYPE = "_raydriver._tcp.local."


def _local_ip() -> str:
    try:
        return socket.gethostbyname(socket.gethostname())
    except OSError:
        return "127.0.0.1"


def _announce_address(config: Any) -> str:
    """The address clients should connect to, matching the bind.

    A loopback bind must advertise loopback (not the hostname's
    address, which may differ, e.g. 127.0.1.1); a wildcard bind
    advertises the LAN address; a specific bind advertises itself.
    """
    host = getattr(config, "host", "127.0.0.1")
    if is_loopback(host):
        return "127.0.0.1"
    if host in ("0.0.0.0", "::", "*", ""):
        return _local_ip()
    return host


class Announcer:
    """A registered mDNS service; close() unregisters it."""

    def __init__(self, zeroconf: Any, info: Any) -> None:
        self._zeroconf = zeroconf
        self.info = info

    async def close(self) -> None:
        await self._zeroconf.async_unregister_service(self.info)
        self._zeroconf.close()


async def announce(config: Any, port: int) -> Announcer:
    """Announce the daemon on the LAN so clients discover it."""
    zeroconf = Zeroconf()
    name = f"raydriver-{socket.gethostname()}.{SERVICE_TYPE}"
    info = ServiceInfo(
        SERVICE_TYPE,
        name,
        addresses=[socket.inet_aton(_announce_address(config))],
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
    """Browse for daemons for *timeout* seconds. Each entry has name,
    host, port and txt fields."""
    found: set[str] = set()

    class _Listener(ServiceListener):
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
                        key.decode(): value.decode() if value else ""
                        for key, value in info.properties.items()
                    },
                }
            )
    return results
