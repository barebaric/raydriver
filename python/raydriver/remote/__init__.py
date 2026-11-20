"""Remote sessions: a device-host daemon and a client proxy.

The implementation lives in Rust; the submodules here are thin
delegates so ``raydriver.remote.protocol`` and friends keep their
historical import paths.
"""

import raydriver.raydriver as _raydriver
from raydriver.remote import client, daemon, inspect, mdns, protocol

__all__ = ["client", "daemon", "inspect", "mdns", "protocol"]


def __getattr__(name):
    return getattr(_raydriver.remote, name)
