"""Device census for GRBL controllers.

Thin Python shim over the Rust census in ``raydriver.grbl.census``:
the session sends ``$I`` and ``$$``, and the native ``describe()``
turns the responses into a :class:`GrblDeviceInfo`.

A local :class:`~raydriver.grbl.GrblSession`, an ESP3D-backed
session and a remote daemon proxy are probed identically.
"""

from __future__ import annotations

from typing import Protocol

from raydriver.grbl.census import (
    GrblDeviceInfo,
    GrblFlavor,
    describe,
)

__all__ = [
    "GrblDeviceInfo",
    "GrblFlavor",
    "describe",
    "probe_device",
]


class _ProbeSession(Protocol):
    """The session surface the census needs."""

    async def execute_interactive_command(self, command: str) -> list[str]: ...


async def probe_device(session: _ProbeSession) -> GrblDeviceInfo:
    """Query a connected session for ``$I`` and ``$$`` and return
    the census."""
    build_info = await session.execute_interactive_command("$I")
    settings_lines = await session.execute_interactive_command("$$")
    return describe(build_info, settings_lines)
