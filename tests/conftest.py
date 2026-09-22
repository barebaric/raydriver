"""Shared fixtures: sessions driven against the GRBL firmware
emulator (raydriver.emulator)."""

import asyncio
import faulthandler
import time

import pytest

from raydriver.emulator import GrblEmulator
from raydriver.grbl import GrblSession, MockTransport


def _arm_freeze_watchdog():
    """Dump all thread stacks and exit if the process runs too long.

    The dump and the exit run on a C-level thread and work even when
    the GIL is stuck (unlike Python timers, which need the GIL to
    wake).  Healthy runs finish in minutes; a frozen process dies at
    the deadline, leaving `freeze-dump.txt` for CI to persist.
    """
    dump_file = open("freeze-dump.txt", "w")
    faulthandler.dump_traceback_later(600, exit=True, file=dump_file)


_arm_freeze_watchdog()

FAST_TIMINGS = {
    "handshake_timeout": 2.0,
    "handshake_poll_interval": 0.02,
    "status_poll_interval": 0.05,
    "reconnect_delay": 0.2,
    "command_timeout": 2.0,
    "poll_response_interval": 0.01,
    "poll_response_attempts": 10,
    "safety_shutdown_delay": 0.02,
    # Generous stall margins: sandboxed CI runners can pause the
    # event loop for hundreds of milliseconds, and tripping a drain
    # timeout would run deadlock recovery (which resets the pending
    # queue) before a late ack arrives.
    "stall_timeout_default": 3.0,
    "stall_timeout_min": 1.0,
    "stall_timeout_max": 5.0,
}

DEFAULT_DIALECT = {
    "home_all": "$H",
    "home_axis": "$H{axis_letter}",
    "move_to": "$J=G90 G21 F{speed} X{x} Y{y}",
    "jog": "$J=G91 G21 F{speed}",
    "clear_alarm": "$X",
    "laser_on": "M4 S{power:.0f}",
    "laser_off": "M5",
    "focus_laser_on": "M3 S{power:.0f}",
    "tool_change": "T{tool_number}",
    "set_wcs_offset": "G10 L2 P{p_num} X{x} Y{y} Z{z}",
    "probe_cycle": "G38.2 {axis_letter}{max_travel} F{feed_rate}",
    "safety_off_commands": ["M5", "M9"],
}


class EventRecorder:
    def __init__(self):
        self.events = []

    def callback(self, name, payload=None):
        self.events.append((name, payload))

    def names(self):
        return [name for name, _ in self.events]

    async def wait_for(self, name, pred=None, timeout=5.0):
        """Wait until an event *name* (matching *pred*) was recorded."""
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            for event_name, payload in self.events:
                if event_name != name:
                    continue
                if pred is None or pred(payload):
                    return payload
            await asyncio.sleep(0.01)
        raise AssertionError(
            f"event {name!r} not observed within {timeout:.1f}s; "
            f"got {self.names()}"
        )


def make_session(mock, events, config=None, dialect=None):
    config = {**FAST_TIMINGS, **(config or {})}
    return GrblSession.with_transport(
        mock,
        config=config,
        dialect={**DEFAULT_DIALECT, **(dialect or {})},
        event_callback=events.callback,
    )


async def wait_until(predicate, timeout=5.0, interval=0.01):
    """Poll *predicate* until it returns a truthy value."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        await asyncio.sleep(interval)
    raise AssertionError(f"condition not met within {timeout:.1f}s")


@pytest.fixture
def events():
    return EventRecorder()


@pytest.fixture
def mock():
    return MockTransport()


async def start_device(mock, **kwargs):
    """Start an emulator task and return (emulator, task)."""
    emulator = GrblEmulator(mock, **kwargs)
    task = asyncio.ensure_future(emulator.run())
    return emulator, task


@pytest.fixture
async def rig(mock, events):
    """A connected session running against the firmware emulator.

    Yields ``(session, mock, emulator, events)`` with the handshake
    complete and the sent log cleared.
    """
    emulator, device_task = await start_device(mock)
    session = make_session(mock, events)
    await session.connect()
    await events.wait_for(
        "connection_status_changed", lambda p: p[0] == "CONNECTED"
    )
    mock.clear_sent()
    events.events.clear()
    try:
        yield session, mock, emulator, events
    finally:
        await session.disconnect()
        device_task.cancel()
        try:
            await device_task
        except asyncio.CancelledError:
            pass


def sent_bytes(session_mock):
    return b"".join(session_mock.sent())
