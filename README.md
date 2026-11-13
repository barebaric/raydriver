# raydriver

Rust-native machine drivers for [Rayforge](https://github.com/barebaric/rayforge),
exposed to Python via PyO3.

Currently this provides a full port of Rayforge's advanced GRBL
serial driver: character-counting flow control, handshake and status
polling, interactive command queue, job streaming with stall detection
and deadlock recovery, cancel/safety-shutdown semantics, settings and
WCS access, and probe cycles. Dialects (command templates) remain data
owned by Rayforge and are passed in resolved form.

## CLI

The `raydriver` command runs G-code against a real device with live
progress, or against the built-in firmware emulator for hardware-free
dry runs:

```bash
raydriver run job.gcode --port /dev/ttyUSB0 --baudrate 115200
raydriver run job.gcode --emulator          # no hardware needed
raydriver status --port /dev/ttyUSB0 --seconds 5
```

While a job runs, `raydriver run` shows acknowledged lines, a
progress bar, the device state, machine position, feed rate and ETA;
`Ctrl-C` cancels the job and runs the safety shutdown.

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
(protocols: `grbl`, `grbl_simple`, `smoothie`, `marlin`).
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
protocols), and decoders plug in per protocol (`grbl` ships first;
`raw` and `hex` cover the rest).

```bash
# Record a session (or --emulator for a hardware-free dry run)
raydriver analyzer record --port /dev/ttyUSB0 --out cap.jsonl
raydriver analyzer tui --port /dev/ttyUSB0     # interactive monitor

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
from raydriver.grbl import GrblSession, SessionEvent

session = GrblSession(
    config={"port": "/dev/ttyUSB0", "baudrate": 115200},
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
