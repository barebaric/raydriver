"""Device census for GRBL controllers.

Turns the raw ``$I`` and ``$$`` response lines into a structured
:class:`GrblDeviceInfo`: firmware identity, compile flags, the parsed
settings inventory, and the semantic values applications need (work
travel, motion limits, laser mode, unit reporting). Pure functions
over parser output plus one session helper — no I/O of its own, so a
local :class:`~raydriver.grbl.GrblSession`, an ESP3D-backed session
and a remote daemon proxy are probed identically.
"""

from __future__ import annotations

import enum
import re
from collections.abc import Sequence
from dataclasses import dataclass
from typing import Protocol

from raydriver.grbl.parser import (
    extract_device_name,
    parse_grbl_settings,
    parse_opt_flags,
    parse_opt_info,
    parse_version,
)

__all__ = [
    "GrblDeviceInfo",
    "GrblFlavor",
    "describe",
    "probe_device",
]


class GrblFlavor(enum.Enum):
    """The firmware family a device identifies itself as."""

    STOCK = "stock"
    GRBLHAL = "grblhal"
    FLUIDNC = "fluidnc"
    UNKNOWN = "unknown"


_HAL_RE = re.compile(r"grblhal", re.IGNORECASE)
_FLUIDNC_RE = re.compile(r"fluidnc", re.IGNORECASE)

#: grblHAL exposes per-axis settings under the ``$4xx`` group
#: (e.g. ``$400`` for axis steps/mm). Stock Grbl uses ``$0``–``$255``.
_HAL_SETTING_RE = re.compile(r"\$4\d{2}")


class _ProbeSession(Protocol):
    """The session surface the census needs.

    Satisfied by every GRBL-family session: the local Rust
    sessions and the remote daemon proxy alike.
    """

    async def execute_interactive_command(self, command: str) -> list[str]: ...


@dataclass(frozen=True)
class GrblDeviceInfo:
    """The structured result of probing a GRBL device."""

    name: str
    firmware_version: str | None
    rx_buffer_size: int | None
    compile_flags: str
    flavor: GrblFlavor
    settings: dict[str, float]
    max_travel: tuple[float, float] | None
    max_speed: int | None
    acceleration: int | None
    max_spindle: int | None
    laser_mode: bool | None
    homing_enabled: bool | None
    report_inches: bool | None
    arc_tolerance: float | None
    single_axis_homing: bool


def _detect_flavor(
    build_info: Sequence[str], settings_lines: Sequence[str]
) -> GrblFlavor:
    """Classify the firmware family from ``$I`` and ``$$`` output.

    Signals:
    - ``[VER:]`` contains ``grblHAL`` or ``FluidNC`` → the named flavor
    - ``$$`` includes ``$4xx`` grouped axis settings → ``GRBLHAL``
    - Stock Grbl 1.1 (parseable version, no HAL signals) → ``STOCK``
    - Unrecognized / empty → ``UNKNOWN``
    """
    for line in build_info:
        if _HAL_RE.search(line):
            return GrblFlavor.GRBLHAL
        if _FLUIDNC_RE.search(line):
            return GrblFlavor.FLUIDNC

    if any(_HAL_SETTING_RE.search(line) for line in settings_lines):
        return GrblFlavor.GRBLHAL

    if parse_version(build_info) is not None:
        return GrblFlavor.STOCK

    return GrblFlavor.UNKNOWN


def describe(
    build_info: Sequence[str], settings_lines: Sequence[str]
) -> GrblDeviceInfo:
    """Build the census from raw Grbl ``$I`` and ``$$`` lines.

    Pure data transformation with no I/O. The caller is responsible
    for communicating with the device and passing the collected
    response lines.
    """
    rx_buffer_size: int | None = None
    compile_flags: str = ""
    for line in build_info:
        rx = parse_opt_info(line)
        if rx is not None:
            rx_buffer_size = rx
        flags = parse_opt_flags(line)
        if flags is not None:
            compile_flags = flags

    settings = parse_grbl_settings(settings_lines)

    axis_x = settings.get("130")
    axis_y = settings.get("131")
    max_x_rate = settings.get("110")
    max_y_rate = settings.get("111")
    x_accel = settings.get("120")
    y_accel = settings.get("121")
    max_spindle = settings.get("30")
    laser_mode = settings.get("32")
    homing_enabled = settings.get("22")
    report_inches = settings.get("13")

    max_speed: int | None = None
    if max_x_rate is not None and max_y_rate is not None:
        max_speed = int(min(max_x_rate, max_y_rate))

    acceleration: int | None = None
    if x_accel is not None and y_accel is not None:
        acceleration = int(min(x_accel, y_accel))

    max_travel: tuple[float, float] | None = None
    if axis_x is not None and axis_y is not None:
        max_travel = (float(axis_x), float(axis_y))

    return GrblDeviceInfo(
        name=extract_device_name(build_info),
        firmware_version=parse_version(build_info),
        rx_buffer_size=rx_buffer_size,
        compile_flags=compile_flags,
        flavor=_detect_flavor(build_info, settings_lines),
        settings=settings,
        max_travel=max_travel,
        max_speed=max_speed,
        acceleration=acceleration,
        max_spindle=(int(max_spindle) if max_spindle is not None else None),
        laser_mode=(bool(int(laser_mode)) if laser_mode is not None else None),
        homing_enabled=(
            bool(int(homing_enabled)) if homing_enabled is not None else None
        ),
        report_inches=(
            bool(int(report_inches)) if report_inches is not None else None
        ),
        arc_tolerance=settings.get("12"),
        single_axis_homing="H" in compile_flags,
    )


async def probe_device(session: _ProbeSession) -> GrblDeviceInfo:
    """Query a connected session for ``$I`` and ``$$`` and return
    the census."""
    build_info = await session.execute_interactive_command("$I")
    settings_lines = await session.execute_interactive_command("$$")
    return describe(build_info, settings_lines)
