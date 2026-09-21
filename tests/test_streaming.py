"""Job streaming tests: backpressure, stalls, alarms, cancel."""

import asyncio
import time

from conftest import make_session


def gcode_lines(n, prefix="G1 X"):
    return "\n".join(f"{prefix}{i}" for i in range(n))


class TestRunBasics:
    async def test_simple_job_completes(self, connected, events):
        session, mock, device, events = connected
        await session.run("G1 X10\nG1 Y5\n", {0: 0, 1: 1}, [0.1, 0.1])
        await events.wait_for("job_finished")
        await asyncio.sleep(0.05)
        assert session.buffer_count == 0
        assert session.pending_commands == []

    async def test_job_sends_all_lines(self, connected):
        session, mock, device, events = connected
        await session.run("G1 X1\nG1 X2\nG1 X3\n")
        await asyncio.sleep(0.2)
        sent = b"".join(mock.sent())
        assert b"G1 X1\n" in sent
        assert b"G1 X2\n" in sent
        assert b"G1 X3\n" in sent

    async def test_progress_callbacks_fire_per_op(self, connected):
        session, mock, device, events = connected
        done = []
        gcode = "G1 X1\nG1 X2\nG1 X3\nG1 X4\n"
        await session.run(gcode, {0: 0, 1: 0, 3: 1}, [], done.append)
        await asyncio.sleep(0.1)
        # The first ack completes op 0; the last ack completes op 1.
        assert done == [0, 1]

    async def test_comments_and_empty_lines_skipped(self, connected):
        session, mock, device, events = connected
        await session.run("G1 X1 ; comment\n\n(comment only)\nG1 X2\n")
        await asyncio.sleep(0.2)
        sent = b"".join(mock.sent())
        assert b"comment" not in sent
        assert b"G1 X1\n" in sent
        assert b"G1 X2\n" in sent

    async def test_job_status_reflected_as_run(self, connected, events):
        session, mock, device, events = connected
        device.ack_delay_ticks = 5
        job = asyncio.ensure_future(session.run("G1 X1\n"))
        await events.wait_for(
            "state_changed", lambda p: p.status.name == "RUN"
        )
        await job
        await events.wait_for("job_finished")


class TestBackpressure:
    async def test_small_buffer_limits_in_flight(self, mock, device, events):
        session = make_session(mock, events, {"rx_buffer_size_override": 30})
        device_task = asyncio.ensure_future(device.run())
        try:
            await session.connect()
            await events.wait_for(
                "connection_status_changed",
                lambda p: p[0] == "CONNECTED",
            )
            mock.clear_sent()
            device.ack_delay_ticks = 20
            gcode = "\n".join(f"G1 X{i}" for i in range(20))
            job = asyncio.ensure_future(session.run(gcode))
            await asyncio.sleep(0.5)
            # The streamer must not overflow the 30-byte RX buffer.
            assert session.buffer_count <= 30
            device.ack_delay_ticks = 0
            await asyncio.wait_for(job, timeout=10)
            await events.wait_for("job_finished")
            await asyncio.sleep(0.05)
            assert session.buffer_count == 0
        finally:
            await session.disconnect()
            device_task.cancel()


class TestErrorsDuringJob:
    async def test_error_halts_stream(self, mock, device, events):
        session = make_session(mock, events)
        device_task = asyncio.ensure_future(device.run())
        try:
            await session.connect()
            await events.wait_for(
                "connection_status_changed",
                lambda p: p[0] == "CONNECTED",
            )
            mock.clear_sent()
            device.auto_ok = False
            # More lines than fit the buffer: the streamer parks
            # waiting for space, so the error provably halts it.
            gcode = "\n".join(f"G1 X{i}" for i in range(30))
            job = asyncio.ensure_future(session.run(gcode))
            await asyncio.sleep(0.3)
            assert not job.done()
            mock.push(b"error:20\r\n")
            await asyncio.wait_for(job, timeout=5)
            await events.wait_for("job_finished")
            await asyncio.sleep(0.1)
            sent = b"".join(mock.sent())
            assert sent.count(b"G1 X") < 30
            assert b"\x18" in sent
            states = [p for n, p in events.events if n == "state_changed"]
            assert any(
                s.error is not None and s.error.code == 20 for s in states
            )
        finally:
            await session.disconnect()
            device_task.cancel()

    async def test_alarm_status_halts_stream(self, mock, device, events):
        session = make_session(mock, events)
        device_task = asyncio.ensure_future(device.run())
        try:
            await session.connect()
            await events.wait_for(
                "connection_status_changed",
                lambda p: p[0] == "CONNECTED",
            )
            mock.clear_sent()
            device.auto_ok = False
            # The stall callback sees the ALARM status and aborts.
            device.status = b"<Alarm:2|MPos:0.000,0.000,0.000>\r\n"
            gcode = "\n".join(f"G1 X{i}" for i in range(30))
            job = asyncio.ensure_future(session.run(gcode))
            await asyncio.wait_for(job, timeout=10)
            await events.wait_for("job_finished")
            await asyncio.sleep(0.1)
            sent = b"".join(mock.sent())
            assert sent.count(b"G1 X") < 30
            assert b"\x18" in sent
        finally:
            await session.disconnect()
            device_task.cancel()

    async def test_alarm_line_halts_stream(self, connected, events):
        session, mock, device, events = connected
        device.ack_delay_ticks = 30  # acks land at ~300 ms

        async def inject_alarm():
            await asyncio.sleep(0.15)
            mock.push(b"ALARM:1\r\n")

        asyncio.ensure_future(inject_alarm())
        await session.run("G1 X1\nG1 X2\nG1 X3\n")
        await events.wait_for("job_finished")
        await asyncio.sleep(0.1)
        assert b"\x18" in b"".join(mock.sent())


class TestStallBehavior:
    async def test_stall_aborts_when_device_dies(self, mock, device, events):
        session = make_session(mock, events)
        device_task = asyncio.ensure_future(device.run())
        try:
            await session.connect()
            await events.wait_for(
                "connection_status_changed",
                lambda p: p[0] == "CONNECTED",
            )
            mock.clear_sent()
            # Device answers nothing at all from now on.
            device.silent = True
            gcode = "\n".join(f"G1 X{i}" for i in range(50))
            await asyncio.wait_for(session.run(gcode), timeout=15.0)
            await events.wait_for("job_finished")
            # Only the first RX-buffer-full batch was sent.
            sent = b"".join(mock.sent())
            assert sent.count(b"G1 X") < 50
        finally:
            await session.disconnect()
            device_task.cancel()

    async def test_stall_retries_while_device_alive(
        self, mock, device, events
    ):
        session = make_session(mock, events)
        device_task = asyncio.ensure_future(device.run())
        try:
            await session.connect()
            await events.wait_for(
                "connection_status_changed",
                lambda p: p[0] == "CONNECTED",
            )
            mock.clear_sent()
            # The device answers polls with Run status but never
            # acks: the streamer must keep waiting (retry forever),
            # not abort.
            from conftest import STATUS_RUN

            device.status = STATUS_RUN
            device.auto_ok = False
            gcode = "\n".join(f"G1 X{i}" for i in range(50))
            job = asyncio.ensure_future(session.run(gcode))
            await asyncio.sleep(1.5)
            assert not job.done()
            # Now let the device drain: ack everything outstanding.
            device.auto_ok = True
            mock.push(b"ok\r\n" * 50)
            await asyncio.wait_for(job, timeout=10)
            await events.wait_for("job_finished")
        finally:
            await session.disconnect()
            device_task.cancel()


class TestCancel:
    async def test_cancel_sends_soft_reset_and_safety_shutdown(
        self, connected, events
    ):
        session, mock, device, events = connected
        device.ack_delay_ticks = 30
        asyncio.ensure_future(session.run("G1 X1\nG1 X2\n"))
        await asyncio.sleep(0.2)
        mock.clear_sent()
        await session.cancel()
        await events.wait_for("job_finished")
        await asyncio.sleep(0.1)
        sent = b"".join(mock.sent())
        assert b"\x18" in sent
        assert b"M5\n" in sent
        assert b"M9\n" in sent

    async def test_cancel_without_job(self, connected):
        session, mock, device, events = connected
        mock.clear_sent()
        await session.cancel()
        await asyncio.sleep(0.1)
        sent = b"".join(mock.sent())
        assert b"\x18" in sent
        assert b"M5\n" in sent

    async def test_cancel_emergency_adds_estop(self, connected):
        session, mock, device, events = connected
        session.update_dialect({"emergency_stop": "M112"})
        mock.clear_sent()
        await session.cancel(emergency=True)
        await asyncio.sleep(0.1)
        sent = b"".join(mock.sent())
        assert b"M112\n" in sent

    async def test_cancelled_sender_does_not_resurrect(
        self, mock, device, events
    ):
        """Issue #428 regression: after cancel(), a sender parked on a
        full buffer must never resume streaming the cancelled job."""
        session = make_session(mock, events)
        device_task = asyncio.ensure_future(device.run())
        try:
            await session.connect()
            await events.wait_for(
                "connection_status_changed",
                lambda p: p[0] == "CONNECTED",
            )
            mock.clear_sent()
            device.status = (
                b"<Run|MPos:0,0,0|FS:100,0>\r\n"  # alive, never idle
            )
            device.auto_ok = False
            gcode = "\n".join(f"G1 X{i}" for i in range(50))
            job = asyncio.ensure_future(session.run(gcode))
            await asyncio.sleep(0.5)
            assert not job.done()
            await session.cancel()
            await asyncio.sleep(0.2)
            finished_at = time.monotonic()
            assert job.done()

            # Late acks and free buffer must not resurrect the job.
            device.auto_ok = True
            mock.push(b"ok\r\n" * 50)
            await asyncio.sleep(0.5)

            def data_sent():
                # Ignore the ongoing realtime status polls.
                return b"".join(c for c in mock.sent() if c not in (b"?",))

            sent_after = data_sent()
            await asyncio.sleep(0.3)
            assert data_sent() == sent_after
            sent_lines = sent_after.count(b"G1 X")
            assert sent_lines < 50

            # Commands work again after the cancelled job wound down.
            lines = await asyncio.wait_for(
                session.execute_command("$X"), timeout=5
            )
            assert lines == ["ok"]
            assert time.monotonic() - finished_at < 10
        finally:
            await session.disconnect()
            device_task.cancel()


class TestRunRaw:
    async def test_realtime_commands_bypass_stream(self, connected):
        session, mock, device, events = connected
        mock.clear_sent()
        await session.run_raw("?")
        await asyncio.sleep(0.1)
        sent = b"".join(mock.sent())
        assert b"?" in sent
        # A bare realtime command must not be queued as gcode.
        assert sent.count(b"\n") == 0

    async def test_mixed_raw(self, connected):
        session, mock, device, events = connected
        mock.clear_sent()
        await session.run_raw("G1 X1\n!\nG1 X2\n")
        await asyncio.sleep(0.2)
        sent = b"".join(mock.sent())
        assert b"G1 X1\n" in sent
        assert b"!" in sent
        assert b"G1 X2\n" in sent
