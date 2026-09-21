# AGENTS.md

## Available commands

- `make dev` — build and install into the active venv
- `make stubs` — re-generate `.pyi` type stubs (after changing `src/python/` bindings)
- `make lint` — lint all code (Rust + Python)
- `make lint-rust` — lint Rust only
- `make lint-python` — lint Python only
- `make format` — auto-format all code (Rust + Python, including PEP8 import ordering)
- `make format-rust` — format Rust only
- `make format-python` — format Python only (ruff handles formatting and import sorting)
- `make test` — run the full test suite. make sure to "make dev" before you test
- `make check` — lint + test
- `make build` — build the wheel (release)

## Rules

- You are strictly forbidden from editing stubs manually. They are only
  to be edited using "make stubs".
- Use make commands when available - avoid calling the underlying tools
  directly.
- Never add Rust tests (`#[cfg(test)]` / `#[test]` blocks in `src/`).
  All tests are Python-based, under `tests/`. Exercise new Rust code
  through PyO3 bindings from Python test code.
- Keep the Grbl protocol behavior byte-for-byte compatible with the
  Python driver in Rayforge
  (`rayforge/machine/driver/grbl/grbl_serial.py`). Do not change
  protocol quirks (ack extraction order, realtime bypass, buffer
  accounting, stall/liveness rules) without an explicit request.

## Layering Rules Specification

The crate is split into two layers that depend only downward:

```
grbl (protocol + session core)  →  python (PyO3 bindings)
```

- `src/grbl/` must not depend on PyO3 or any Python types. It is pure
  Rust: protocol parsing, flow control, transports, and the session
  state machines. Python-facing notifications go through the
  `SessionEvents` trait; the `python` feature provides the
  implementation that marshals events across the boundary.
- `src/python/` contains all PyO3 bindings and may use `src/grbl/`
  freely, never the other way around.

## Export Policy: Explicit Paths

Every item has exactly one canonical path — its leaf module. Parent
mod.rs must not re-export children's items (no namespace flattening).

Exceptions: Primitive types, errors, classes and constants that are
_public_ AND _shared_ within a submodule.

Python sub-modules mirror the Rust hierarchy - no aliases or re-exports
at higher levels. The compiled module is `raydriver.raydriver`; the
pure-Python packages under `python/raydriver/` serve as the importable
packages and delegate to the Rust module via module-level
`__getattr__`:

```python
import raydriver.raydriver as _raydriver

def __getattr__(name):
    return getattr(_raydriver.grbl, name)
```

Rust submodules registered with `add_submodule` are intentionally NOT
registered in `sys.modules` — the Python `__init__.py` of the same
name is the package.

## Python/Rust Boundary Rules

- No domain model objects cross the Python/Rust boundary — only
  primitive types, typed pyclasses defined in this crate, and
  JSON-serialisable dicts.
- Async Rust methods return asyncio futures that are completed from
  the tokio side via `call_soon_threadsafe` on the caller's event
  loop. Never block the loop inside a binding.
- Events emitted by the session are delivered to the Python
  `event_callback(name: str, payload)` on the event loop thread.
  Bindings must not call the callback from a foreign thread directly.

## Dialects

Dialects remain data owned by Rayforge. This crate receives resolved
command templates (a plain dict of format strings) and only needs the
subset used for interactive commands. Formatting supports the Python
`str.format` subset `{name}` and `{name:.Nf}`.
