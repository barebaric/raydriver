"""Raydriver — Rust-native machine drivers for Rayforge.

Rust-native machine drivers exposed via PyO3.  Drivers live entirely
in Rust (protocol parsing, flow control, transports, session state
machines); the pure-Python packages under ``raydriver`` delegate to
the compiled module.

Submodules
----------
- raydriver.grbl — GRBL device sessions, transports and value types
"""

import raydriver.raydriver as _raydriver  # type: ignore[import-untyped]


def __getattr__(name):
    return getattr(_raydriver, name)


__all__ = [
    "grbl",
]
