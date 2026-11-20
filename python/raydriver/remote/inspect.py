"""Read-only inspection of a running daemon."""

import raydriver.raydriver as _raydriver  # type: ignore[import-untyped]

_inspect = _raydriver.remote
daemon_rpc = _inspect.daemon_rpc
snapshot = _inspect.snapshot
welcome_probe = _inspect.welcome_probe

__all__ = ["daemon_rpc", "snapshot", "welcome_probe"]
