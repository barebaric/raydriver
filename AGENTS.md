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
- `make check-wasm` — `cargo check` for the pure-Rust core on the
  native target and on `wasm32-unknown-unknown` (no features)
- `make docs` — generate the API reference from the `.pyi` stubs
- `make build` — build the wheel (release)

## CLI

- `raydriver run job.gcode --port /dev/ttyUSB0` — stream a G-code
  file to a real device with live progress
- `raydriver run job.gcode --emulator` — same, against the built-in
  firmware emulator (no hardware needed)
- `raydriver status --port /dev/ttyUSB0` — connect and print live
  status reports

## Testing

All tests are Python-based, under `tests/`, and run against
`raydriver.grbl.emulator.GrblEmulator` — a GRBL 1.1h *firmware emulator*
(not a mock): it models the character-counting RX buffer, the
15-block planner with deferred acknowledgements, realtime command
interception, modal G-code state, feed-rate-timed motion, alarms,
homing, probing and the `$` system commands.  Device-side code
consumes driver writes via `MockTransport.take_new_sent()` (a
monotonic cursor — never index into `sent()`, which tests may clear
at any time).

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

The crate is split into layers that depend only downward:

```
encode (machine-code encoders; raygeo core only)
core (transports, value types, dialects, driver primitives)
  →  grbl | smoothie | marlin | esp3d | octoprint | xtool | epilog  (drivers)
  →  driver (the typed built-in driver list and resource lock)
  →  python (PyO3 bindings)
```

- `src/core/` is protocol-neutral and pure Rust: byte transports,
  device value types, dialect command-template formatting, the
  `DriverEvents`/`DriverError`/`AsyncEvent` driver primitives, the
  shared ping-pong line-driver skeleton (`core::line_driver`), the
  typed `DriverInfo`, and the transport-level discovery scanners.
  Protocol knowledge does not live here: the remote daemon's driver
  table and fingerprints are supplied by the embedder
  (`core::remote::registry::Registry`).
- `src/encode/` holds the machine-code encoders (the G-code dialect
  encoder and the Epilog print-job encoder). They implement
  `raygeo::ops::convert::Encoder`, depend on raygeo's pure-Rust core
  only, and must stay free of PyO3. The `python` feature exposes each
  spec through the `__raygeo_encoder__()` capsule handoff, so a
  raygeo pipeline can run an encoder raygeo does not define. The
  capsule name embeds the raygeo version the encoder was built
  against; raygeo rejects a mismatch instead of misreading it.
- `src/grbl/`, `src/smoothie/`, `src/marlin/`, `src/esp3d/`,
  `src/octoprint/`, `src/xtool/` (with `m2`) and `src/epilog/` hold
  the drivers; they must not depend on PyO3 or any Python types.
  They may use `src/core/` freely. Cross-protocol imports between
  protocol modules need an explicit justification in the importing
  module's `mod.rs` docs (e.g. Smoothie reuses the GRBL parser and
  error tables because Smoothie emits Grbl-style reports; esp3d is a
  transport used by `GrblNetworkDriver`).
- `src/driver/` sits above the protocol modules: it holds the typed
  `DriverKind` list of built-ins, their construction from a JSON-style
  config, and the process-wide resource lock. It must not use PyO3.
- `src/python/` contains all PyO3 bindings and may use `src/core/`
  and the protocol modules freely, never the other way around.

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

Sanctioned exceptions: `python/raydriver/esp3d/__init__.py` exposes
only the ESP3D connection params (`Esp3dConfig`; the driver is
`raydriver.grbl.GrblNetworkDriver`), `python/raydriver/driver/__init__.py`
adds the common base accessor, and `python/raydriver/remote/__init__.py`
binds its submodule attributes eagerly.

Rust *parent* modules (`grbl`, `core`, ...) are intentionally NOT
registered in `sys.modules` — the Python `__init__.py` of the same
name is the package. Rust *leaf* submodules (`grbl.probe`,
`core.discovery.*`, ...) ARE registered via `attach_submodule`, so
imports like
`from raydriver.grbl.probe import describe` resolve to the compiled
module and must not be shadowed by a same-named Python file.

The `probe` modules are native (`grbl`, `marlin`, `smoothie`,
`octoprint`). Each exports the protocol's census class, `describe()`
and a `probe_device(driver)` compatibility wrapper that forwards to
the driver's native `probe_device()` method. Do not add a Python
`probe.py` next to them: it would shadow the compiled module.

## The Driver Hierarchy

Every protocol driver is one Rust struct implementing the `Driver`
trait (`core::driver`) and one Python subclass of the single
`raydriver.driver.Driver` base class, declared with PyO3 `extends`.

Rules:

- The uniform surface is declared exactly once, on `PyDriver`. Never
  redeclare a base method on a subclass; subclass bindings keep only
  the constructor, protocol-specific extras (`probe_device`,
  `probe_info`, `execute_block`, `start`, `upload`) and `__repr__`.
- No inheritance between protocol drivers: the GRBL variants are
  sibling structs sharing `grbl::stream` through composition.
- Driver differences live in the Rust `Driver` impls. An operation a
  protocol cannot perform returns `DriverError::Unsupported`, which
  the bindings surface as `NotImplementedError`; the method is always
  present.
- Identity, capabilities, WCS set, config keys and the auto-detection
  fingerprint live in each driver's typed `DriverInfo`
  (`pub static INFO`). `src/driver::DriverKind` is the only list of
  built-ins. There is no protocol registry and no method-name table.
- Class names are the persisted driver names (`GrblSerialDriver`,
  `MarlinSerialDriver`, ...). Do not rename or alias them without a
  migration path for saved machines.
- Signatures: `run(gcode, op_line_map=None, op_estimates=None,
  progress_callback=None)`, `home(axes=None, active_wcs=None)`,
  `move_to(speed, x, y, z=None)`, `jog(speed, deltas=None)`,
  `cancel(emergency=False)`, and so on; 3rd-party callers may rely on
  positional order.

## Python/Rust Boundary Rules

- No domain model objects cross the Python/Rust boundary — only
  primitive types, typed pyclasses defined in this crate, and
  JSON-serialisable dicts.
- Async Rust methods return asyncio futures that are completed from
  the tokio side via `call_soon_threadsafe` on the caller's event
  loop. Never block the loop inside a binding.
- Events emitted by the driver are delivered to the Python
  `event_callback(DriverEvent)` on the event loop thread. The drivers
  build one `core::driver::DriverEvent`; the bindings convert it to
  the Python enum and the remote wire event. Bindings must not call
  the callback from a foreign thread directly.
- Cancelling the asyncio future returned by `run()`/`run_raw()`
  aborts the Rust future and invokes the driver cancel path; it is
  not equivalent to merely dropping the Python task.

## Dialects

Dialects remain data owned by Rayforge. This crate receives resolved
command templates (a plain dict of format strings) and only needs the
subset used for interactive commands. Formatting supports the Python
`str.format` subset `{name}` and `{name:.Nf}`.

## Build Features and WASM

The crate compiles for hosts *and* for `wasm32-unknown-unknown` —
and wasm keeps everything a browser can plausibly drive: driver
logic, the HTTP/WS clients, the telnet transport and the
esp3d/octoprint/xtool drivers all run there, with dialing going
through the injectable `core::connector` seam (see docs/wasm.md).
Cargo features exist only for what a browser physically cannot do;
verify with `make check-wasm`.

| Feature | Gates | Why it cannot run on wasm |
|---|---|---|
| `net` | `discovery::{daemon,probe,subnet}`, `remote::{client,inspect}` | raw TCP/UDP sockets (device-facing dialing is deliberately **not** gated — it uses the connector seam) |
| `mdns` | `discovery::mdns`, `remote::mdns` | UDP multicast + OS threads |
| `serial` | tokio-serial transport impl | OS file descriptors (Web Serial replaces it; port *enumeration* via `serialport` stays available without the feature) |
| `remote-daemon` | `remote::daemon` | TCP listener, TLS certs from the filesystem; implies `net` |
| `m2` | `xtool::m2` | tokio-tungstenite has no wasm build; implies `net` |
| `tls-native-roots` / `tls-webpki-roots` | the M2 WebSocket client's TLS trust store (exactly one is required by `m2`) | no OS trust store on wasm; `m2` is off there |
| `python` | `src/python/` | Implies all desktop features + tokio `rt-multi-thread` |

Rules:

- `default = ["net", "mdns", "serial", "remote-daemon", "m2",
  "tls-native-roots"]`; a wasm build is `--no-default-features` with
  **no** features.
- `m2` needs exactly one TLS root provider: `tls-native-roots` (the
  default — the OS store, so administrator-installed CAs work and
  `webpki-roots` stays out of the dependency graph) or
  `tls-webpki-roots` (bundled Mozilla roots).  Both imply `m2`; a
  `--no-default-features --features m2` build without either fails at
  compile time with an explanatory error.
- Device-facing network code (HTTP/WS clients, telnet transport,
  esp3d/octoprint/xtool drivers) dials through
  `core::connector::dial` and must compile unconditionally; on hosts
  without TCP sockets the embedder installs a `StreamConnector`.
  Never reintroduce `tokio::net::TcpStream` there.
- Feature-gated transports keep their canonical module path in every
  build (`#[path = "..._unsupported.rs"]` stubs fail at runtime), so
  `TransportKind::build` needs no cfg arms.
- `tokio::net` / `rt-multi-thread` / `spawn_blocking` on the wasm
  path, or new wasm-hostile dependencies (`mio`, `socket2`,
  `getrandom`) — all forbidden without a cfg gate and an
  `unsupported` fallback (see `transport/bluetooth/` for the
  pattern: platform-gated backends + `is_supported()`).
- New host-only code goes behind one of the features above;
  everything else compiles everywhere. The `encode` module is in that
  "everywhere" set: raygeo's core is wasm-clean and is an
  unconditional dependency.
