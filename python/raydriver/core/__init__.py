"""Shared device primitives: transports, value types and session
events.

The protocol namespaces (``raydriver.grbl``, ``raydriver.smoothie``,
``raydriver.marlin``) build on these.
"""

import raydriver.raydriver as _raydriver  # type: ignore[import-untyped]


def __getattr__(name):
    return getattr(_raydriver.core, name)
