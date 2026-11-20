"""Protocol registry: the single source of truth for what each
session protocol speaks and what it can do.

The registry is implemented in Rust and exposed through the compiled
module; this package is its stable import path, plus the
``PROTOCOLS`` name-to-metadata mapping. The daemon hosts sessions by
protocol; a client (or an application such as Rayforge) learns a
protocol's label, capabilities and config shape from here — as info
— instead of hardcoding protocol knowledge.
"""

import raydriver.raydriver as _raydriver  # type: ignore[import-untyped]

ProtocolInfo = _raydriver.ProtocolInfo
protocol_info = _raydriver.protocol_info
protocols = _raydriver.protocols
protocols_payload = _raydriver.protocols_payload

PROTOCOLS = protocols()

__all__ = [
    "PROTOCOLS",
    "ProtocolInfo",
    "protocol_info",
    "protocols",
    "protocols_payload",
]


def __getattr__(name):
    return getattr(_raydriver, name)
