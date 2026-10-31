"""Golden captures: one deterministic scenario per protocol, run
against the firmware emulators and diffed against committed
fixtures (regenerate with RAYDRIVER_REGEN_GOLDEN=1).

These prove the analyzer pipeline end to end — recorder, capture,
decoder, listing — and pin each protocol's wire behavior: a change
in what a session sends or a firmware emulator answers shows up as
a fixture diff.  The same harness accepts captures from real
hardware as fixtures.
"""

import asyncio
import time
from pathlib import Path

from conftest import DEFAULT_DIALECT, FAST_TIMINGS, EventRecorder

from raydriver.analyzer.capture import Recorder
from raydriver.analyzer.golden import assert_capture_matches_golden
from raydriver.core import SessionEvent
from raydriver.core.transport import MockTransport
from raydriver.grbl import GrblSession
from raydriver.grbl.emulator import GrblEmulator
from raydriver.marlin import MarlinSession
from raydriver.marlin.emulator import MarlinEmulator
from raydriver.smoothie import SmoothieSession
from raydriver.smoothie.emulator import SmoothieEmulator

GOLDEN_DIR = Path(__file__).parent / "golden"

SMOOTHIE_MARLIN_TIMINGS = {
    "handshake_timeout": 2.0,
    "handshake_poll_interval": 0.02,
    "status_poll_interval": 60.0,
    "reconnect_delay": 0.2,
    "command_timeout": 2.0,
}


async def _await_connected(events):
    await events.wait_for(
        SessionEvent.ConnectionStatusChanged,
        lambda event: event.status == "CONNECTED",
    )


async def _quiesce(recorder, quiet=0.1, timeout=5.0):
    """Wait until no new frames arrive for *quiet* seconds, so the
    handshake's trailing frames land before scripted commands do
    (their order otherwise races across platforms)."""
    deadline = time.monotonic() + timeout
    last_count = -1
    last_change = time.monotonic()
    while time.monotonic() < deadline:
        count = len(recorder.capture.frames)
        if count != last_count:
            last_count = count
            last_change = time.monotonic()
        elif time.monotonic() - last_change >= quiet:
            return
        await asyncio.sleep(0.01)
    raise AssertionError("traffic did not quiesce")


async def grbl_scenario():
    mock = MockTransport()
    emulator = GrblEmulator(mock, speed_factor=2000.0)
    device_task = asyncio.ensure_future(emulator.run())
    events = EventRecorder()
    recorder = Recorder(forward=events.callback)
    session = GrblSession.with_transport(
        mock,
        config={
            **FAST_TIMINGS,
            "poll_status_while_running": False,
            # The status poll loop runs whenever the session is
            # connected; silent it so listings depend on the script,
            # not on how many polls fit into the scenario.
            "status_poll_interval": 60.0,
        },
        dialect=DEFAULT_DIALECT,
        event_callback=recorder,
    )
    recorder.capture.protocol = "grbl"
    recorder.capture.source = "golden scenario: grbl emulator"
    try:
        await session.connect()
        await _await_connected(events)
        await _quiesce(recorder)
        await session.execute_interactive_command("$I")
        await session.execute_interactive_command("$G")
        await session.run("G90\nG1 X10 F1000\n", {0: 0, 1: 1}, [], None)
        await session.disconnect()
    finally:
        device_task.cancel()
    return recorder.capture


async def smoothie_scenario():
    mock = MockTransport()
    emulator = SmoothieEmulator(mock)
    device_task = asyncio.ensure_future(emulator.run())
    events = EventRecorder()
    recorder = Recorder(forward=events.callback)
    session = SmoothieSession.with_transport(
        mock,
        config=SMOOTHIE_MARLIN_TIMINGS,
        dialect={},
        event_callback=recorder,
    )
    recorder.capture.protocol = "smoothie"
    recorder.capture.source = "golden scenario: smoothie emulator"
    try:
        await session.connect()
        await _await_connected(events)
        await _quiesce(recorder)
        await session.execute_interactive_command("version")
        await session.run("G1 X10 F6000\nG1 Y5 F6000\n")
        await session.disconnect()
    finally:
        device_task.cancel()
    return recorder.capture


async def marlin_scenario():
    mock = MockTransport()
    emulator = MarlinEmulator(mock)
    device_task = asyncio.ensure_future(emulator.run())
    events = EventRecorder()
    recorder = Recorder(forward=events.callback)
    session = MarlinSession.with_transport(
        mock,
        config={**SMOOTHIE_MARLIN_TIMINGS, "connect_settle": 0.05},
        dialect={},
        event_callback=recorder,
    )
    recorder.capture.protocol = "marlin"
    recorder.capture.source = "golden scenario: marlin emulator"
    try:
        await session.connect()
        await _await_connected(events)
        await _quiesce(recorder)
        await session.execute_interactive_command("M115")
        await session.run("G1 X10 F90000\nG1 Y5 F90000\n")
        await session.disconnect()
    finally:
        device_task.cancel()
    return recorder.capture


async def test_golden_grbl():
    capture = await grbl_scenario()
    assert len(capture.frames) > 10
    assert_capture_matches_golden(
        "grbl-session", capture, golden_dir=GOLDEN_DIR
    )


async def test_golden_smoothie():
    capture = await smoothie_scenario()
    assert len(capture.frames) > 5
    assert_capture_matches_golden(
        "smoothie-session", capture, golden_dir=GOLDEN_DIR
    )


async def test_golden_marlin():
    capture = await marlin_scenario()
    assert len(capture.frames) > 5
    assert_capture_matches_golden(
        "marlin-session", capture, golden_dir=GOLDEN_DIR
    )
