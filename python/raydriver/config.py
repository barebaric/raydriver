"""Typed session configuration objects.

The typed configs are frozen pyclasses implemented in Rust: they
spell out the transport and timing fields for each protocol and
reject typos at type-checking time. This package is their stable
import path and keeps the ``SessionConfig`` base for annotations.
"""

from __future__ import annotations

from typing import Any

import raydriver.raydriver as _raydriver  # type: ignore[import-untyped]

Esp3dConfig = _raydriver.Esp3dConfig
GrblSerialConfig = _raydriver.GrblSerialConfig
GrblSessionOptions = _raydriver.GrblSessionOptions
GrblSimpleConfig = _raydriver.GrblSimpleConfig
GrblTelnetConfig = _raydriver.GrblTelnetConfig
M2Config = _raydriver.M2Config
MarlinConfig = _raydriver.MarlinConfig
OctoPrintConfig = _raydriver.OctoPrintConfig
RemoteEndpoint = _raydriver.RemoteEndpoint
SmoothieConfig = _raydriver.SmoothieConfig
XtoolConfig = _raydriver.XtoolConfig


class SessionConfig:
    """Base for the typed session configuration classes.

    Annotation shell only: the concrete configs are Rust pyclasses
    satisfying the same attribute surface, consumed by the session
    constructors through ``to_dict()``.
    """

    def to_dict(self) -> dict[str, Any]:
        """The mapping form expected by the session constructors."""
        ...


__all__ = [
    "Esp3dConfig",
    "GrblSerialConfig",
    "GrblSessionOptions",
    "GrblSimpleConfig",
    "GrblTelnetConfig",
    "M2Config",
    "MarlinConfig",
    "OctoPrintConfig",
    "RemoteEndpoint",
    "SessionConfig",
    "SmoothieConfig",
    "XtoolConfig",
]


def __getattr__(name):
    return getattr(_raydriver, name)
