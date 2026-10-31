"""The Smoothie session: strict send-then-ack ordering, status
reports, probing queries, hold/resume and cancellation.

The strict-ordering tests drive a scripted MockTransport without the
emulator, releasing acknowledgements one by one; the behavioral tests
run against the Smoothie firmware emulator."""

import asyncio

import pytest
from conftest import DEFAULT_DIALECT, sent_bytes, wait_until

from raydriver.core import SessionEvent
from raydriver.core.types import DeviceStatus
from raydriver.smoothie import SmoothieSession
from raydriver.smoothie.emulator import SmoothieEmulator

GCODE = "G1 X1 F1000\nG1 X2 F1000\nG1 X3 F1000\n"

TIMINGS = {
    "handshake_timeout": 2.0,
    "handshake_poll_interval": 0.02,
    "status_poll_interval": 0.05,
    "reconnect_delay": 0.2,
    "command_timeout": 2.0,
}

STATUS = b"<Idle|MPos:0.000,0.000,0.000|WPos:0.000,0.000,0.000|FS:0,0>\r\n"


def gcode_sends(mock):
    """Command lines written by the session (status polls
    excluded)."""
    return [chunk for chunk in mock.sent() if chunk.startswith(b"G1 ")]


def make_smoothie_session(mock, events, config=None):
    return SmoothieSession.with_transport(
        mock,
        config={**TIMINGS, **(config or {})},
        dialect=DEFAULT_DIALECT,
        event_callback=events.callback,
    )


async def connect_scripted(mock, events):
    """Connect a session to a mute device that only speaks when the
    test scripts it (no emulator)."""
    session = make_smoothie_session(mock, events)
    connect_task = asyncio.ensure_future(session.connect())
    await wait_until(lambda: mock.sent())
    mock.push(STATUS)
    await connect_task
    await events.wait_for(
        SessionEvent.ConnectionStatusChanged,
        lambda event: event.status == "CONNECTED",
    )
    mock.clear_sent()
    events.events.clear()
    return session


class TestStrictOrdering:
    async def test_next_line_waits_for_ack(self, mock, events):
        session = await connect_scripted(mock, events)
        job = asyncio.ensure_future(session.run(GCODE))

        await wait_until(lambda: len(gcode_sends(mock)) >= 1)
        await asyncio.sleep(0.3)
        assert len(gcode_sends(mock)) == 1

        mock.push(b"ok\r\n")
        await wait_until(lambda: len(gcode_sends(mock)) >= 2)
        await asyncio.sleep(0.3)
        assert len(gcode_sends(mock)) == 2

        mock.push(b"ok\r\n")
        await wait_until(lambda: len(gcode_sends(mock)) >= 3)
        mock.push(b"ok\r\n")

        await asyncio.wait_for(job, 5)
        await events.wait_for(SessionEvent.JobFinished)
        await session.disconnect()

    async def test_device_error_halts_stream(self, mock, events):
        session = await connect_scripted(mock, events)
        job = asyncio.ensure_future(session.run(GCODE))
        await wait_until(lambda: len(gcode_sends(mock)) >= 1)
        mock.push(b"ok\r\n")
        await wait_until(lambda: len(gcode_sends(mock)) >= 2)
        mock.push(b"error:20\r\n")

        await asyncio.wait_for(job, 5)
        await events.wait_for(SessionEvent.JobFinished)
        await asyncio.sleep(0.2)
        assert len(gcode_sends(mock)) == 2
        assert session.state.error is not None
        await session.disconnect()

    async def test_cancel_sends_ctrl_c_and_safety_off(self, mock, events):
        session = await connect_scripted(mock, events)
        job = asyncio.ensure_future(session.run(GCODE))
        await wait_until(lambda: len(gcode_sends(mock)) >= 1)

        await session.cancel()
        await asyncio.wait_for(job, 5)
        await events.wait_for(SessionEvent.JobFinished)
        sent = sent_bytes(mock)
        assert b"\x03" in sent
        assert b"M5\n" in sent
        await session.disconnect()

    async def test_hold_sends_realtime_feed_hold(self, mock, events):
        session = await connect_scripted(mock, events)
        mock.clear_sent()
        await session.set_hold(True)
        assert b"!" in sent_bytes(mock)
        assert session.state.status == DeviceStatus.HOLD
        await session.set_hold(False)
        assert b"~" in sent_bytes(mock)
        await session.disconnect()


@pytest.fixture
async def smoothie_rig(mock, events):
    """A connected session running against the Smoothie firmware
    emulator."""
    emulator = SmoothieEmulator(mock)
    device_task = asyncio.ensure_future(emulator.run())
    session = make_smoothie_session(mock, events)
    await session.connect()
    await events.wait_for(
        SessionEvent.ConnectionStatusChanged,
        lambda event: event.status == "CONNECTED",
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


class TestEmulatorJobs:
    async def test_job_completes_and_moves(self, smoothie_rig):
        session, mock, emulator, events = smoothie_rig
        await session.run("G1 X10 F6000\nG1 Y5 F6000\n")
        await events.wait_for(SessionEvent.JobFinished)
        await asyncio.sleep(0.1)
        assert emulator.mpos == pytest.approx([10.0, 5.0, 0.0])

    async def test_status_reports_update_state(self, smoothie_rig):
        session, mock, emulator, events = smoothie_rig
        assert session.state.status == DeviceStatus.IDLE

    async def test_progress_callbacks_fire_per_op(self, smoothie_rig):
        session, mock, emulator, events = smoothie_rig
        emulator.speed_factor = 50
        done = []
        text = "\n".join(f"G1 X{i % 50} F6000" for i in range(40))
        op_map = {**{i: 0 for i in range(20)}, 30: 1}
        await session.run(text, op_map, done.append)
        await events.wait_for(SessionEvent.JobFinished, timeout=30)
        assert done == [0, 1]

    async def test_probe_queries(self, smoothie_rig):
        session, mock, emulator, events = smoothie_rig
        version = await session.execute_interactive_command("version")
        assert version[0].startswith("Build version:")
        assert version[-1] == "ok"
        alpha_max = await session.execute_interactive_command(
            "config-get alpha_max"
        )
        assert alpha_max == ["200.0000", "ok"]

    async def test_hold_parks_job_until_resume(self, smoothie_rig):
        session, mock, emulator, events = smoothie_rig
        emulator.speed_factor = 100
        text = "\n".join(f"G1 X{50 if i % 2 else 0} F600" for i in range(30))
        job = asyncio.ensure_future(session.run(text))
        await wait_until(lambda: gcode_sends(mock))

        await session.set_hold(True)
        await asyncio.sleep(1.0)
        assert session.job_running
        assert not job.done()
        assert len(gcode_sends(mock)) < 30

        await session.set_hold(False)
        await asyncio.wait_for(job, 60)
        await events.wait_for(SessionEvent.JobFinished, timeout=30)

    async def test_cancel_aborts_job(self, smoothie_rig):
        session, mock, emulator, events = smoothie_rig
        emulator.speed_factor = 2
        text = "\n".join(f"G1 X{50 if i % 2 else 0} F600" for i in range(30))
        job = asyncio.ensure_future(session.run(text))
        await asyncio.sleep(0.5)
        assert not job.done()

        await session.cancel()
        await asyncio.wait_for(job, 5)
        await events.wait_for(SessionEvent.JobFinished)
        assert b"\x03" in sent_bytes(mock)
