"""Transport-level device discovery scanners.

Serial port capture, mDNS service browse and subnet TCP scanning,
implemented in the Rust core. Interpreting scan output — deciding
which driver could talk to a device — lives with the embedding
application's drivers.
"""

import raydriver.raydriver as _raydriver  # type: ignore[import-untyped]


def __getattr__(name):
    return getattr(_raydriver.core.discovery, name)
