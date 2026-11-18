"""Device census for Marlin controllers.

Turns the raw ``M115``, ``M211``, ``M503`` and ``M149`` response
lines into a structured :class:`MarlinDeviceInfo`: firmware identity,
software-endstop travel and motion limits. Pure functions over parser
output plus one session helper — no I/O of its own, so a local
:class:`~raydriver.marlin.MarlinSession` and a remote daemon proxy
are probed identically.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Protocol

from raydriver.marlin.parser import (
    detect_unit_system_from_m149,
    extract_marlin_device_name,
    is_ok_response,
    parse_m115_firmware_info,
    parse_m211_endstops,
    parse_m503_settings,
    parse_marlin_version,
)

__all__ = [
    "MarlinDeviceInfo",
    "describe",
    "probe_device",
]

#: Commands the census queries, in order.
CENSUS_COMMANDS = ("M115", "M211", "M503", "M149")


class _ProbeSession(Protocol):
    """The session surface the census needs.

    Satisfied by the local Rust session and the remote daemon
    proxy alike.
    """

    async def execute_interactive_command(self, command: str) -> list[str]: ...


@dataclass(frozen=True)
class MarlinDeviceInfo:
    """The structured result of probing a Marlin device."""

    name: str
    firmware_version: str | None
    machine_type: str | None
    max_travel: tuple[float, float] | None
    max_speed: int | None
    acceleration: int | None
    units: str | None


def _responses(
    lines_by_command: Mapping[str, Sequence[str]],
) -> dict[str, list[str]]:
    """Collect the census responses, dropping ``ok`` lines.

    The parsers match semantic lines only; ``ok`` acknowledgements
    carry no census data.
    """
    return {
        command: [
            line
            for line in lines_by_command[command]
            if not is_ok_response(line)
        ]
        for command in CENSUS_COMMANDS
    }


def describe(
    m115_lines: Sequence[str],
    m211_lines: Sequence[str],
    m503_lines: Sequence[str],
    m149_lines: Sequence[str],
    boot_lines: Sequence[str] | None = None,
) -> MarlinDeviceInfo:
    """Build the census from raw Marlin census-command responses.

    Pure data transformation with no I/O. The caller is responsible
    for communicating with the device and passing the collected
    response lines. *boot_lines* are startup messages observed
    before the handshake; a version found there is more specific
    than the ``FIRMWARE_NAME`` token M115 reports.
    """
    boot = boot_lines or []

    name = extract_marlin_device_name(m115_lines, boot)

    fw_info = parse_m115_firmware_info(m115_lines)
    fw_name = fw_info.get("firmware_name", "")
    if fw_name:
        for line in boot:
            ver = parse_marlin_version(line)
            if ver:
                fw_name = ver
                break

    max_travel: tuple[float, float] | None = None
    endstops = parse_m211_endstops(m211_lines)
    if endstops is not None:
        x_max, y_max = endstops
        if x_max > 0 and y_max > 0:
            max_travel = (x_max, y_max)

    m503 = parse_m503_settings(m503_lines)

    max_speed: int | None = None
    max_feed_x = m503.get("max_feedrate_x")
    max_feed_y = m503.get("max_feedrate_y")
    if max_feed_x is not None and max_feed_y is not None:
        max_speed = int(min(max_feed_x, max_feed_y) * 60)

    acceleration: int | None = None
    accel_val = m503.get("acceleration")
    if accel_val is not None:
        acceleration = int(accel_val)

    return MarlinDeviceInfo(
        name=name,
        firmware_version=fw_name or None,
        machine_type=fw_info.get("machine_type") or None,
        max_travel=max_travel,
        max_speed=max_speed,
        acceleration=acceleration,
        units=detect_unit_system_from_m149(m149_lines),
    )


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
    lines_by_command = {
        command: await session.execute_interactive_command(command)
        for command in CENSUS_COMMANDS
    }
    responses = _responses(lines_by_command)
    return describe(
        responses["M115"],
        responses["M211"],
        responses["M503"],
        responses["M149"],
        boot_lines,
    )
