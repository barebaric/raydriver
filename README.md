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
from raydriver.grbl import GrblSession

events = []
session = GrblSession(
    config={"port": "/dev/ttyUSB0", "baudrate": 115200},
    dialect={
        "home_all": "$H",
        "home_axis": "$H{axis_letter}",
        "clear_alarm": "$X",
        "laser_on": "M4 S{power:.0f}",
        "laser_off": "M5",
    },
    event_callback=lambda name, payload: events.append((name, payload)),
)

async def main():
    await session.connect()
    print(await session.execute_interactive_command("$I"))
    await session.disconnect()

asyncio.run(main())
```
