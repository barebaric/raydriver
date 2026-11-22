"""Device census for Marlin controllers.

Thin Python shim over the Rust census in ``raydriver.marlin.census``:
the session runs the census commands, and the native ``describe()``
turns the responses into a :class:`MarlinDeviceInfo`.
"""

from __future__ import annotations

from collections.abc import Sequence
from typing import Protocol

from raydriver.marlin.census import MarlinDeviceInfo, describe

__all__ = [
    "MarlinDeviceInfo",
    "describe",
    "probe_device",
]

#: Commands the census queries, in order.
CENSUS_COMMANDS = ("M115", "M211", "M503", "M149")


class _ProbeSession(Protocol):
    """The session surface the census needs."""

    async def execute_interactive_command(self, command: str) -> list[str]: ...


async def probe_device(
    session: _ProbeSession,
    boot_lines: Sequence[str] | None = None,
) -> MarlinDeviceInfo:
    """Query a connected session with the census commands and
    return the census.

    *boot_lines* are the startup messages the caller observed on
    the session (e.g. from ``RawRx`` events), used to refine the
    firmware version.
    """
    responses = {
        command: await session.execute_interactive_command(command)
        for command in CENSUS_COMMANDS
    }
    return describe(
        responses["M115"],
        responses["M211"],
        responses["M503"],
        responses["M149"],
        boot_lines,
    )
