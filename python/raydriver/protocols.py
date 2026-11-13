"""Protocol registry: the single source of truth for what each
session protocol speaks and what it can do.

The daemon hosts sessions by protocol; a client (or an application
such as Rayforge) learns a protocol's label, capabilities and config
shape from here — as info — instead of hardcoding protocol knowledge.
Nothing in this module imports the session classes, so it is safe to
use from both the daemon and the client.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

#: Methods every session implements, regardless of protocol.
COMMON_METHODS = frozenset(
    {
        "connect",
        "disconnect",
        "run",
        "run_raw",
        "cancel",
        "execute_command",
        "execute_interactive_command",
        "home",
        "move_to",
        "jog",
        "select_tool",
        "set_power",
        "set_focus_power",
        "set_wcs_offset",
        "update_dialect",
    }
)

#: Properties every session exposes.
COMMON_PROPERTIES = frozenset({"state", "job_running", "resource_uri"})


@dataclass(frozen=True)
class ProtocolInfo:
    """Metadata describing one session protocol.

    ``capabilities`` is derived from the method/property surface so
    the two never drift apart. ``dialect_family`` is a hint clients
    can map to their own command-dialect definitions; it is not a
    driver selection.
    """

    name: str
    label: str
    dialect_family: str
    connection: str
    config_keys: tuple[str, ...]
    methods: frozenset[str]
    properties: frozenset[str]

    @property
    def capabilities(self) -> dict[str, bool]:
        return {
            "uses_gcode": True,
            "supports_settings": "read_settings" in self.methods,
            "supports_probing": "run_probe_cycle" in self.methods,
            "supports_hold": "set_hold" in self.methods,
            "supports_clear_alarm": "clear_alarm" in self.methods,
            "supports_unit_detection": "detect_unit_system" in self.methods,
            "reports_buffer_stats": "buffer_count" in self.properties,
        }

    def to_dict(self) -> dict[str, Any]:
        """The wire form served by ``protocol/list``."""
        return {
            "name": self.name,
            "label": self.label,
            "dialect_family": self.dialect_family,
            "connection": self.connection,
            "config_keys": list(self.config_keys),
            "capabilities": self.capabilities,
            "methods": sorted(self.methods),
            "properties": sorted(self.properties),
        }


_GRBL_EXTRA = frozenset(
    {
        "set_hold",
        "read_settings",
        "write_setting",
        "read_wcs_offsets",
        "read_parser_state",
        "run_probe_cycle",
        "detect_unit_system",
    }
)

PROTOCOLS: dict[str, ProtocolInfo] = {
    "grbl": ProtocolInfo(
        name="grbl",
        label="GRBL",
        dialect_family="grbl",
        connection="serial_or_telnet",
        config_keys=("port", "baudrate", "host", "tcp_port"),
        methods=COMMON_METHODS | _GRBL_EXTRA,
        properties=COMMON_PROPERTIES
        | frozenset({"buffer_count", "rx_buffer_size", "pending_commands"}),
    ),
    "grbl_simple": ProtocolInfo(
        name="grbl_simple",
        label="GRBL (simple)",
        dialect_family="grbl",
        connection="serial_or_telnet",
        config_keys=("port", "baudrate", "host", "tcp_port"),
        methods=COMMON_METHODS | _GRBL_EXTRA,
        properties=COMMON_PROPERTIES,
    ),
    "smoothie": ProtocolInfo(
        name="smoothie",
        label="Smoothieware",
        dialect_family="smoothieware",
        connection="telnet",
        config_keys=("host", "tcp_port"),
        methods=COMMON_METHODS | frozenset({"set_hold", "clear_alarm"}),
        properties=COMMON_PROPERTIES,
    ),
    "marlin": ProtocolInfo(
        name="marlin",
        label="Marlin",
        dialect_family="marlin",
        connection="serial",
        config_keys=("port", "baudrate"),
        methods=COMMON_METHODS
        | frozenset({"clear_alarm", "detect_unit_system"}),
        properties=COMMON_PROPERTIES,
    ),
}


def protocol_info(name: str) -> ProtocolInfo:
    """Look up a protocol's metadata, raising ``KeyError`` if unknown."""
    return PROTOCOLS[name]


def protocols_payload() -> dict[str, Any]:
    """The ``protocol/list`` payload: every protocol's metadata."""
    return {"protocols": [info.to_dict() for info in PROTOCOLS.values()]}
