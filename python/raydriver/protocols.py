"""Protocol registry: the single source of truth for what each
session protocol speaks and what it can do.

The registry lives in Rust beside the session bindings; this module
is the thin shim that keeps the import path stable for the daemon,
remote client and embedding applications.
"""

import raydriver.raydriver as _raydriver  # type: ignore[import-untyped]

#: The registry as a name → metadata mapping.
PROTOCOLS = _raydriver.protocols()


def __getattr__(name):
    return getattr(_raydriver, name)
