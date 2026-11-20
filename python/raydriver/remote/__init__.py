"""Remote sessions: a device-host daemon and a client proxy.

The public API is implemented in Rust and exposed through the
compiled extension module; this package only re-exports its
submodules for stable import paths.
"""

import raydriver.raydriver as _raydriver  # type: ignore[import-untyped]

client = _raydriver.remote.client
daemon = _raydriver.remote.daemon
inspect = _raydriver.remote.inspect
mdns = _raydriver.remote.mdns
protocol = _raydriver.remote.protocol

__all__ = ["client", "daemon", "inspect", "mdns", "protocol"]
