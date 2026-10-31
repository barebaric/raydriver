"""The simple ping-pong session: strict send-then-ack ordering,
progress callbacks, mid-job errors, timeouts, hold/resume and
cancellation.

The strict-ordering tests drive a scripted MockTransport without the
emulator, releasing acknowledgements one by one; the behavioral tests
run against the firmware emulator with its planner-deferred acks."""

import asyncio

import pytest
from conftest import (
    DEFAULT_DIALECT,
    FAST_TIMINGS,
    sent_bytes,
    start_device,
    wait_until,
)

from raydriver.core import SessionEvent
from raydriver.grbl.simple import GrblSimpleSession

GCODE = "G1 X1 F1000\nG1 X2 F1000\nG1 X3 F1000\n"


def gcode_sends(mock):
    """G-code lines written by the session (status polls excluded)."""
    return [chunk for chunk in mock.sent() if chunk.startswith(b"G1 ")]


def make_simple_session(mock, events, config=None):
    return GrblSimpleSession.with_transport(
        mock,
        config={**FAST_TIMINGS, **(config or {})},
        dialect=DEFAULT_DIALECT,
        event_callback=events.callback,
    )


async def wait_status_polls(mock, count=2):
    """Wait for the periodic '?' polls, which only start once the
    handshake completed."""
    await wait_until(lambda: mock.sent().count(b"?") >= count)


async def connect_scripted(mock, events, config=None):
    """Connect a simple session to a mute device that only speaks
    when the test scripts it (no emulator auto-acking)."""
    session = make_simple_session(mock, events, config)
    connect_task = asyncio.ensure_future(session.connect())
    # The first handshake poll only gets recorded once the transport
    # is open (push before open() would be dropped).
    await wait_until(lambda: mock.sent())
    mock.push(b"Grbl 1.1h ['$' for help]\r\n")
    await connect_task
    await events.wait_for(
        SessionEvent.ConnectionStatusChanged,
        lambda event: event.status == "CONNECTED",
    )
    await wait_status_polls(mock)
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

    async def test_timeout_interrupts_job_with_safety_shutdown(
        self, mock, events
    ):
        session = await connect_scripted(
            mock, events, config={"command_timeout": 0.3}
        )
        job = asyncio.ensure_future(session.run(GCODE))
        await wait_until(lambda: len(gcode_sends(mock)) >= 1)

        # The device never acknowledges: the hard timeout must
        # interrupt the job and fire the safety shutdown.
        await asyncio.wait_for(job, 5)
        await events.wait_for(SessionEvent.JobFinished)
        await asyncio.sleep(0.2)
        assert len(gcode_sends(mock)) == 1
        assert b"M5\n" in sent_bytes(mock)
        await session.disconnect()

    async def test_cancel_aborts_waiting_sender(self, mock, events):
        session = await connect_scripted(mock, events)
        job = asyncio.ensure_future(session.run(GCODE))
        await wait_until(lambda: len(gcode_sends(mock)) >= 1)

        # Line 2 is never acknowledged; cancel must abort the wait.
        await session.cancel()
        await asyncio.wait_for(job, 5)
        await events.wait_for(SessionEvent.JobFinished)
        await asyncio.sleep(0.2)
        assert len(gcode_sends(mock)) == 1
        assert b"\x18" in sent_bytes(mock)
        await session.disconnect()


@pytest.fixture
async def simple_rig(mock, events):
    """A connected simple session running against the firmware
    emulator."""
    emulator, device_task = await start_device(mock)
    session = make_simple_session(mock, events)
    await session.connect()
    await events.wait_for(
        SessionEvent.ConnectionStatusChanged,
        lambda event: event.status == "CONNECTED",
    )
    await wait_status_polls(mock)
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
    async def test_job_completes_and_moves(self, simple_rig):
        session, mock, emulator, events = simple_rig
        await session.run("G1 X10 F1000\nG1 Y5 F1000\n")
        await events.wait_for(SessionEvent.JobFinished)
        await asyncio.sleep(0.1)
        assert emulator.mpos == pytest.approx([10.0, 5.0, 0.0])

    async def test_progress_callbacks_fire_per_op(self, simple_rig):
        session, mock, emulator, events = simple_rig
        emulator.speed_factor = 50
        done = []
        text = "\n".join(f"G1 X{i % 50} F1000" for i in range(40))
        op_map = {**{i: 0 for i in range(20)}, 30: 1}
        await session.run(text, op_map, done.append)
        await events.wait_for(SessionEvent.JobFinished, timeout=30)
        assert done == [0, 1]

    async def test_interactive_error_returned_as_line(self, simple_rig):
        session, mock, emulator, events = simple_rig
        lines = await session.execute_interactive_command("G999")
        assert lines == ["error:20"]

    async def test_hold_and_resume(self, simple_rig):
        session, mock, emulator, events = simple_rig
        emulator.speed_factor = 50
        # Alternating 50 mm moves: ~100 ms per block, 200 blocks ≈
        # 20 s of motion.  The hold must land long before the job
        # can drain, on slow runners too.
        text = "\n".join(f"G1 X{50 if i % 2 else 0} F600" for i in range(200))
        job = asyncio.ensure_future(session.run(text))
        await wait_until(lambda: gcode_sends(mock))

        await session.set_hold(True)
        await asyncio.sleep(1.0)
        # Held: the planner stops draining, so the sender parks
        # waiting for an ack instead of failing the job.
        assert session.job_running
        assert not job.done()
        assert len(gcode_sends(mock)) < 200

        await session.set_hold(False)
        await asyncio.wait_for(job, 120)
        await events.wait_for(SessionEvent.JobFinished, timeout=60)


class TestConfig:
    def test_unknown_keys_ignored(self, mock, events):
        make_simple_session(mock, events, config={"stall_timeout": 1.0})

    def test_config_requires_transport_details(self, events):
        with pytest.raises(RuntimeError, match="port"):
            GrblSimpleSession(
                None, dialect=None, event_callback=events.callback
            )

    async def test_command_timeout_raises(self, mock, events):
        session = await connect_scripted(
            mock, events, config={"command_timeout": 0.3}
        )
        with pytest.raises(TimeoutError):
            await session.execute_interactive_command("$X")
        await session.disconnect()
