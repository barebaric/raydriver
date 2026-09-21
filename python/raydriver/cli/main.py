"""Command line interface for testing raydriver against devices.

Runs a G-code file against a real GRBL device over serial, or
against the built-in firmware emulator for hardware-free dry runs,
showing live progress while the job executes:

    raydriver run job.gcode --port /dev/ttyUSB0
    raydriver run job.gcode --emulator
    raydriver status --port /dev/ttyUSB0 --seconds 5
"""

import argparse
import asyncio
import sys
import time

from raydriver.grbl import GrblSession, MockTransport
from raydriver.grbl.parser import strip_gcode_comments
from raydriver.grbl.types import DeviceState


def _fmt_seconds(seconds):
    minutes, secs = divmod(int(seconds), 60)
    return f"{minutes:02d}:{secs:02d}"


class RunView:
    """Single-line live progress renderer for a streaming job."""

    def __init__(self, total_lines, out=None):
        self.total = total_lines
        self.out = out
        self.acked = 0
        self.state: DeviceState | None = None
        self.had_error = False
        self.started = time.monotonic()
        self._last_render = 0.0
        self.finished = False

    def _stdout(self):
        return self.out if self.out is not None else sys.stdout

    def progress(self, line_index):
        self.acked = max(self.acked, line_index + 1)
        self.render()

    def on_state(self, state: DeviceState):
        self.state = state
        if state.error is not None:
            self.had_error = True
        self.render()

    def render(self, force=False):
        if self.finished and not force:
            return
        now = time.monotonic()
        if not force and now - self._last_render < 0.05:
            return
        self._last_render = now
        elapsed = now - self.started
        frac = self.acked / self.total if self.total else 1.0
        bar_width = 20
        filled = int(bar_width * frac)
        bar = "#" * filled + "-" * (bar_width - filled)
        parts = [f"\r {self.acked}/{self.total} [{bar}] {frac:6.1%}"]
        if self.state is not None:
            mpos = ",".join(
                f"{v:.3f}" if v is not None else "  -  "
                for v in self.state.machine_pos
            )
            state = self.state.status.name
            feed = (
                f" F{self.state.feed_rate}"
                if self.state.feed_rate is not None
                else ""
            )
            parts.append(f"{state} MPos:{mpos}{feed}")
        if frac > 0:
            eta = elapsed / frac - elapsed
            parts.append(f"eta {_fmt_seconds(eta)}")
        parts.append(f"{_fmt_seconds(elapsed)}")
        line = " | ".join(parts)
        print(
            f"{line:<100}",
            end="",
            flush=True,
            file=self._stdout(),
        )

    def finish(self, success, message=""):
        self.finished = False
        self.render(force=True)
        self.finished = True
        elapsed = time.monotonic() - self.started
        print(file=self._stdout())
        status = "done" if success else f"ABORTED — {message}"
        print(
            f" {status}: {self.acked}/{self.total} lines acknowledged "
            f"in {_fmt_seconds(elapsed)}",
            file=self._stdout(),
        )


def _open_session(args, on_event):
    poll = not getattr(args, "no_poll", False)
    if args.emulator:
        from raydriver.emulator import GrblEmulator

        mock = MockTransport()
        emulator = GrblEmulator(mock, speed_factor=args.speed_factor)
        session = GrblSession.with_transport(
            mock,
            config={"poll_status_while_running": poll},
            dialect={},
            event_callback=on_event,
        )
        return session, asyncio.ensure_future(emulator.run())
    return (
        GrblSession(
            config={
                "port": args.port,
                "baudrate": args.baudrate,
                "poll_status_while_running": poll,
            },
            dialect={},
            event_callback=on_event,
        ),
        None,
    )


async def cmd_run(args) -> int:
    with open(args.file) as handle:
        text = handle.read()
    lines = [raw for raw in text.splitlines() if strip_gcode_comments(raw)]
    if not lines:
        print("error: no G-code lines in file", file=sys.stderr)
        return 2

    view = RunView(len(lines))

    connected = asyncio.get_running_loop().create_future()

    def on_event(name, payload=None):
        if name == "state_changed":
            view.on_state(payload)
        elif name == "connection_status_changed" and not (connected.done()):
            if payload[0] == "CONNECTED":
                connected.set_result(True)
            elif payload[0] == "ERROR":
                connected.set_exception(
                    ConnectionError(payload[1] or "connection failed")
                )

    session, device_task = _open_session(args, on_event)
    print(
        "Connecting to "
        + ("emulated device" if args.emulator else args.port)
        + " ..."
    )
    try:
        await session.connect()
        try:
            await asyncio.wait_for(connected, timeout=args.timeout)
        except asyncio.TimeoutError:
            print("error: device did not respond", file=sys.stderr)
            return 1
        info = await session.execute_interactive_command("$I")
        print("Device: " + " | ".join(info))

        op_map = {i: i for i in range(len(lines))}
        await session.run(text, op_map, [], view.progress)
        # Progress can report an errored op as complete (the driver
        # fires every op between two acks), so a device error also
        # marks the run as failed.
        success = view.acked >= len(lines) and not view.had_error
        message = "job did not complete"
        if view.had_error and view.state is not None:
            error = view.state.error
            if error:
                message = f"{error.title}: {error.description}"
        view.finish(success, message)
        return 0 if success else 1
    except asyncio.CancelledError:
        print("\nInterrupted — cancelling job ...")
        await asyncio.shield(session.cancel(emergency=True))
        view.finish(False, "interrupted")
        raise
    finally:
        try:
            await asyncio.shield(session.disconnect())
        except Exception:  # noqa: BLE001 - teardown best effort
            pass
        if device_task is not None:
            device_task.cancel()


async def cmd_status(args) -> int:
    def on_event(name, payload=None):
        if name == "state_changed":
            mpos = ",".join(
                f"{v:8.3f}" if v is not None else "     -  "
                for v in payload.machine_pos
            )
            error = f" ! {payload.error.title}" if payload.error else ""
            print(
                f"{time.strftime('%H:%M:%S')} {payload.status.name:8}"
                f" MPos:{mpos}{error}"
            )

    session, device_task = _open_session(args, on_event)
    try:
        await session.connect()
        await asyncio.sleep(args.seconds)
    finally:
        await session.disconnect()
        if device_task is not None:
            device_task.cancel()
    return 0


def build_parser():
    parser = argparse.ArgumentParser(
        prog="raydriver",
        description="Test raydriver GRBL sessions against real "
        "devices or the built-in firmware emulator.",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    def common(run_parser):
        run_parser.add_argument(
            "--port", default="/dev/ttyUSB0", help="serial port"
        )
        run_parser.add_argument(
            "--baudrate", type=int, default=115200, help="baud rate"
        )
        run_parser.add_argument(
            "--emulator",
            action="store_true",
            help="run against the built-in GRBL firmware emulator "
            "instead of a serial port",
        )
        run_parser.add_argument(
            "--speed-factor",
            type=float,
            default=2000.0,
            help="emulator motion speed multiplier (emulator only)",
        )

    run_parser = subparsers.add_parser(
        "run", help="stream a G-code file to the device"
    )
    common(run_parser)
    run_parser.add_argument("file", help="G-code file to execute")
    run_parser.add_argument(
        "--timeout",
        type=float,
        default=10.0,
        help="seconds to wait for the device to connect",
    )
    run_parser.add_argument(
        "--no-poll",
        action="store_true",
        help="disable status polling during the job",
    )
    run_parser.set_defaults(func=cmd_run)

    status_parser = subparsers.add_parser(
        "status", help="connect and print live status reports"
    )
    common(status_parser)
    status_parser.add_argument(
        "--seconds", type=float, default=5.0, help="duration to watch"
    )
    status_parser.set_defaults(func=cmd_status)
    return parser


async def amain(argv) -> int:
    args = build_parser().parse_args(argv)
    return await args.func(args)


def main() -> None:
    try:
        sys.exit(asyncio.run(amain(sys.argv[1:])))
    except KeyboardInterrupt:
        sys.exit(130)


if __name__ == "__main__":
    main()
