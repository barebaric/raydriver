# raydriver

> **Legacy repository.** raydriver has been merged into
> [raygeo](https://github.com/barebaric/raygeo) and is no longer
> maintained. The drivers live under `raygeo.driver`, the encoders
> under `raygeo.ops.convert`, and the `raydriver` console script is
> still provided by `raygeo`.

## Migrating to raygeo

```sh
pip uninstall raydriver
pip install "raygeo>=2.0.0"
```

| Old path | New path |
| -------- | -------- |
| `raydriver.encode` | `raygeo.ops.convert` |
| `raydriver.driver` | `raygeo.driver` |
| `raydriver.core` | `raygeo.driver.core` |
| `raydriver.grbl` | `raygeo.driver.grbl` |
| `raydriver.marlin` | `raygeo.driver.marlin` |
| `raydriver.smoothie` | `raygeo.driver.smoothie` |
| `raydriver.octoprint` | `raygeo.driver.octoprint` |
| `raydriver.ruida` | `raygeo.driver.ruida` |
| `raydriver.epilog` | `raygeo.driver.epilog` |
| `raydriver.esp3d` | `raygeo.driver.config.Esp3dConfig` |
| `raydriver.xtool` | `raygeo.driver.xtool_d1` |
| `raydriver.xtool.m2` | `raygeo.driver.xtool_m2` |
| `raydriver.remote` | `raygeo.driver.remote` |
| `raydriver.analyzer` | `raygeo.driver.analyzer` |
| `raydriver.simulator` | `raygeo.driver.simulator` |
| any other `raydriver.*` | `raygeo.driver.*` |

The `raydriver` command keeps its name and works the same way once
`raygeo` is installed. Uninstall the old distribution first so its
console script cannot shadow the one from `raygeo`. Daemon config
paths, service names, `_raydriver._tcp`, `_raydriver_protocol` and
`raydriver://` URLs are unchanged.

The rest of this README describes the final standalone release and is
kept for historical reference.

Rust-native machine drivers for [Rayforge](https://github.com/barebaric/rayforge),
exposed to Python via PyO3.

Currently this provides a full port of Rayforge's advanced GRBL
serial driver: character-counting flow control, handshake and status
polling, interactive command queue, job streaming with stall detection
and deadlock recovery, cancel/safety-shutdown semantics, settings and
WCS access, and probe cycles. ESP3D/FluidNC controllers use the same
GRBL session over an HTTP command endpoint and WebSocket response
channel, with upload-and-execute jobs. Dialects (command templates)
remain data owned by Rayforge and are passed in resolved form.
GRBL sessions can also connect over Bluetooth Classic SPP/RFCOMM on
Linux, macOS and Windows, once the device is paired by the operating
system.

## CLI

The `raydriver` command runs G-code against a real device with live
progress, or against the built-in firmware emulator for hardware-free
dry runs:

```bash
raydriver run job.gcode --port /dev/ttyUSB0 --baudrate 115200
raydriver run job.gcode --emulator          # no hardware needed
raydriver status --port /dev/ttyUSB0 --seconds 5
```

Ruida controllers consume rpascript instead of G-code; `run` sends
the whole script and waits for the controller to report idle:

```bash
raydriver run job.rds --protocol ruida --host 192.168.1.50
raydriver run job.rds --protocol ruida --usb-device /dev/ttyUSB0
raydriver run job.rds --protocol ruida --emulator --no-wait
```

While a GRBL job runs, `raydriver run` shows acknowledged lines, a
progress bar, the device state, machine position, feed rate and ETA;
`Ctrl-C` cancels the job and runs the safety shutdown.  The same
cancel path sends `STOP_JOB` to a Ruida controller.

## API reference

The Python API reference under [docs/api](docs/api) is generated from
the `.pyi` type stubs. Regenerate it after changing the Rust bindings:

```bash
make docs
```

## Daemon (remote sessions)

The daemon runs the driver next to the hardware — e.g. on a
Raspberry Pi in the workshop — while Rayforge runs on your desk.
Jobs run daemon-side and survive client disconnects. Works on
Linux, macOS and Windows; full reference in
[docs/daemon.md](docs/daemon.md).

### 1. Add your machine

```bash
pip install raydriver          # includes network discovery
raydriver scan                 # find your machine's serial port
raydriver daemon add /dev/ttyUSB0
```

`add` creates the config file on first use and generates a
**token** — the password clients use to connect. It prints the
token; `raydriver daemon status` shows it again later. The config
lives in `~/.config/raydriver/daemon.json` (Linux),
`~/Library/Application Support/raydriver/daemon.json` (macOS) or
`%APPDATA%\raydriver\daemon.json` (Windows).

The daemon announces itself over mDNS, so Rayforge's machine wizard
discovers it automatically — no manual network setup.

Defaults are GRBL at 115200 baud; for other firmware use
`raydriver daemon add PORT --protocol smoothie --baudrate 115200`
(protocols: `grbl`, `grbl_simple`, `smoothie`, `marlin`,
`octoprint`; ESP3D and FluidNC sessions are configured in
`daemon.json` with their host and HTTP/WebSocket ports). xTool
network sessions are local-only and are not hosted by the daemon.
`raydriver daemon remove PORT` unconfigures a device.

### 2. Run the daemon

```bash
raydriver daemon               # foreground
raydriver daemon install       # or as a service
```

`install` uses a systemd user unit on Linux, a launchd LaunchAgent
on macOS and a logon scheduled task on Windows.
`raydriver daemon status` shows the service state and probes the
daemon; `raydriver daemon uninstall` stops and removes it.

### 3. Check it

```bash
raydriver list                 # configured devices + connection state
raydriver connection           # connected clients + control lease
raydriver info 127.0.0.1:8765  # sessions, devices and cameras
```

### Connecting clients

- **Rayforge**: follow the
  [first-time setup guide](https://rayforge.org/docs/getting-started/first-time-setup)
  on the Rayforge website.
- **Any other raydriver consumer**: connect through the client
  API — see [docs/daemon.md](docs/daemon.md).

### Exposing the daemon to the LAN

On loopback the token never leaves the machine, so no TLS is
needed. Reaching the daemon from another computer requires a
certificate and a non-loopback bind — an exposed listener refuses
to start without both a token and TLS. See
[docs/daemon.md](docs/daemon.md) for the setup.

## Protocol analyzer

The analyzer captures device traffic, decodes it to a readable
listing, and replays or proxies it — the workflow for developing
protocols without hardware and for characterizing real controllers.
Captures are editable JSONL (text lines or base64 for binary
protocols), and decoders plug in per protocol (`grbl` and `ruida`
ship; `raw` and `hex` cover the rest).

```bash
# Record a session (or --emulator for a hardware-free dry run)
raydriver analyzer record --port /dev/ttyUSB0 --out cap.jsonl
raydriver analyzer tui --port /dev/ttyUSB0     # interactive monitor

# Ruida: network, USB or the built-in controller emulator
raydriver analyzer record --protocol ruida --host 192.168.1.50 --out ruida.jsonl
raydriver analyzer record --protocol ruida --usb-device /dev/ttyUSB0 --out ruida.jsonl
raydriver analyzer tui --protocol ruida --emulator

# Print a capture as a decoded listing
raydriver analyzer decode cap.jsonl

# Replay captured frames at a device, at up to Nx speed
raydriver analyzer replay cap.jsonl --to 192.168.1.50:23 --speed 10

# Man-in-the-middle: point an application at the proxy in front of
# a TCP device and record both directions
raydriver analyzer proxy --upstream 192.168.1.50:23 --out mitm.jsonl
```

Golden tests pin wire behavior: deterministic scenarios run
against the firmware emulators, and their decoded listings are
diffed against fixtures under `tests/golden/` (regenerate with
`RAYDRIVER_REGEN_GOLDEN=1`).  Captures from real hardware can be
dropped in as fixtures the same way, so protocol changes that
alter what goes over the wire fail loudly.

### Security policy

Analyzer network endpoints are hardened before any socket opens:

- listeners bind loopback (`127.0.0.1`) without authentication by
  default;
- non-loopback or wildcard binds require **both** a token and TLS
  — there is no override;
- explicitly configured authentication is never dropped.

Token checks and TLS handshakes are covered by tests that drive
the real sockets and assert on the outcomes.

## Releases

Releasing mirrors the Raygeo flow:

1. Configure a PyPI *trusted publisher* for this repository once
   (owner `barebaric`, repo `raydriver`, workflow `release.yml`,
   environment `pypi`).
2. `gh release create v0.1.0 --title "v0.1.0" --generate-notes`

The `Build and Publish` workflow then stamps the version from the
tag, builds wheels (Linux/Windows/macOS) and an sdist, publishes to
PyPI via OIDC, and attaches the artifacts to the GitHub release.

## Development

```bash
make venv && source .venv/bin/activate
make dev     # build + install (do this before testing)
make test    # pytest suite (all tests are Python-based)
make check   # lint + test
```

## Usage

```python
import asyncio
from raydriver.config import GrblSerialConfig
from raydriver.core import SessionEvent
from raydriver.grbl import GrblSession

session = GrblSession(
    config=GrblSerialConfig(port="/dev/ttyUSB0", baudrate=115200),
    dialect={
        "home_all": "$H",
        "home_axis": "$H{axis_letter}",
        "clear_alarm": "$X",
        "laser_on": "M4 S{power:.0f}",
        "laser_off": "M5",
    },
    event_callback=on_event,
)

def on_event(event: SessionEvent):
    match event:
        case SessionEvent.StateChanged():
            print("device state:", event.state.status.name)
        case SessionEvent.ConnectionStatusChanged(status=status):
            print("connection:", status)
        case SessionEvent.RawRx(line=line):
            print("<<", line)
        case SessionEvent.RawTx(line=line):
            print(">>", line)

async def main():
    await session.connect()
    print(await session.execute_interactive_command("$I"))
    await session.disconnect()

asyncio.run(main())
```

### xTool M2

`M2Session` connects to the M2's WS-V2 service for state
monitoring, homing, motion and control of a job already staged on the
machine. It does not upload or stream G-code jobs; use xTool Studio to
stage those. M2 support uses a reverse-engineered local protocol and
has not yet been validated against physical hardware.

The device commonly presents a self-signed TLS certificate. Certificate
validation remains enabled by default. Set `tls_allow_invalid_certs` only
on a trusted private network if the device certificate is not trusted by
the host:

```python
import asyncio
from raydriver.core import SessionEvent
from raydriver.xtool.m2 import M2Session

async def main():
    connected = asyncio.Event()

    def on_event(event):
        if (
            isinstance(event, SessionEvent.ConnectionStatusChanged)
            and event.status == "CONNECTED"
        ):
            connected.set()

    session = M2Session(
        config={
            "host": "192.168.1.50",
            "tls_allow_invalid_certs": True,
        },
        event_callback=on_event,
    )
    await session.connect()
    await asyncio.wait_for(connected.wait(), timeout=15)
    await session.disconnect()

asyncio.run(main())
```

The session also exposes `home`, `move_to`, `jog`, `start`, `set_hold`
and `cancel` for supported machine-control operations. `connect` starts
the connection loop; wait for the `CONNECTED` connection-status event
before issuing machine commands.
