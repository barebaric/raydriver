"""Session lifecycle and interactive command tests (port of
test_grbl_serial_driver.py scenarios that don't involve streaming)."""

import asyncio

import pytest

from raydriver.grbl.types import DeviceStatus


class TestConnection:
    async def test_connect_emits_connected_after_handshake(
        self, mock, device, events
    ):
        session, _, _, _ = None, mock, device, events
        from conftest import make_session

        session = make_session(mock, events)
        device_task = asyncio.ensure_future(device.run())
        try:
            await session.connect()
            payload = await events.wait_for(
                "connection_status_changed",
                lambda p: p[0] == "CONNECTED",
            )
            assert payload[1] is None
            assert b"$I\n" in b"".join(mock.sent())
        finally:
            await session.disconnect()
            device_task.cancel()

    async def test_handshake_timeout_on_phantom_port(self, mock, events):
        from conftest import make_session

        session = make_session(mock, events, {"handshake_timeout": 0.15})
        try:
            await session.connect()
            payload = await events.wait_for(
                "connection_status_changed",
                lambda p: p[0] == "ERROR",
            )
            assert "No response" in payload[1]
            await events.wait_for(
                "connection_status_changed", lambda p: p[0] == "SLEEPING"
            )
        finally:
            await session.disconnect()

    async def test_reconnect_after_recovery(self, mock, events):
        from conftest import FakeDevice, make_session

        session = make_session(mock, events, {"handshake_timeout": 0.15})
        try:
            await session.connect()
            await events.wait_for(
                "connection_status_changed",
                lambda p: p[0] == "ERROR",
            )
            device = FakeDevice(mock)
            device_task = asyncio.ensure_future(device.run())
            await events.wait_for(
                "connection_status_changed",
                lambda p: p[0] == "CONNECTED",
                timeout=5.0,
            )
            device_task.cancel()
        finally:
            await session.disconnect()

    async def test_welcome_variant_grblhal(self, mock, events):
        from conftest import FakeDevice, make_session

        device = FakeDevice(mock, welcome=b"GrblHAL 1.1f\r\n")
        session = make_session(mock, events)
        device_task = asyncio.ensure_future(device.run())
        try:
            await session.connect()
            await events.wait_for(
                "connection_status_changed",
                lambda p: p[0] == "CONNECTED",
            )
        finally:
            await session.disconnect()
            device_task.cancel()

    async def test_command_while_disconnected_raises(self, mock, events):
        from conftest import make_session

        session = make_session(mock, events)
        with pytest.raises(ConnectionError):
            await session.execute_interactive_command("$X")


class TestConnectionState:
    async def test_status_report_updates_state(self, connected, events):
        session, mock, device, events = connected
        from conftest import STATUS_RUN

        mock.push(STATUS_RUN)
        payload = await events.wait_for(
            "state_changed",
            lambda p: p.status == DeviceStatus.RUN,
        )
        assert payload.feed_rate == 500
        assert payload.machine_pos == [1.0, 2.0, 0.0]

    async def test_rx_buffer_size_from_opt(self, mock, events):
        from conftest import FakeDevice, make_session

        device = FakeDevice(mock)
        device.build_info = b"[VER:1.1h:]\r\n[OPT:V,15,356]\r\nok\r\n"
        # Keep Bf out of the status reports so it cannot disagree
        # with the OPT-provided size.
        device.status = b"<Idle|MPos:0.000,0.000,0.000>\r\n"
        session = make_session(mock, events)
        device_task = asyncio.ensure_future(device.run())
        try:
            await session.connect()
            await events.wait_for(
                "config_changed", lambda p: p == ("rx_buffer_size", 356)
            )
            assert session.rx_buffer_size == 356
        finally:
            await session.disconnect()
            device_task.cancel()

    async def test_rx_buffer_size_override_wins(self, mock, device, events):
        from conftest import make_session

        session = make_session(mock, events, {"rx_buffer_size_override": 64})
        device_task = asyncio.ensure_future(device.run())
        try:
            await session.connect()
            await events.wait_for(
                "connection_status_changed",
                lambda p: p[0] == "CONNECTED",
            )
            assert session.rx_buffer_size == 64
        finally:
            await session.disconnect()
            device_task.cancel()

    async def test_fragmented_status_report_ignored(self, connected):
        session, mock, device, events = connected
        states_before = [e for e in events.events if e[0] == "state_changed"]
        mock.push(b"<Idle|MPos")
        await asyncio.sleep(0.1)
        states_after = [e for e in events.events if e[0] == "state_changed"]
        assert len(states_after) == len(states_before)


class TestInteractiveCommands:
    async def test_ok_response(self, connected):
        session, mock, device, events = connected
        lines = await session.execute_interactive_command("$X")
        assert lines == ["ok"]

    async def test_multi_line_response(self, connected):
        session, mock, device, events = connected
        lines = await session.execute_interactive_command("$G")
        assert lines[0].startswith("[G54")
        assert lines[-1] == "ok"

    async def test_error_response(self, connected):
        session, mock, device, events = connected
        device.command_failures["$X"] = "error:2"
        lines = await session.execute_interactive_command("$X")
        assert lines == ["error:2"]

    async def test_queued_command(self, connected):
        session, mock, device, events = connected
        lines = await session.execute_command("$X")
        assert lines == ["ok"]

    async def test_interleaved_status_and_ok(self, connected):
        session, mock, device, events = connected
        # Ack interleaved with a status report fragment, delivered in
        # one chunk: the ack must still be extracted before the line
        # parser sees it.
        fut = asyncio.ensure_future(session.execute_interactive_command("$X"))
        await asyncio.sleep(0.05)
        mock.push(b"<Idle|MPos:0,0,0|Bf:15,127>\r\nok\r\n")
        assert await fut == ["ok"]
        assert session.buffer_count == 0


class TestHold:
    async def test_hold_sends_realtime_and_updates_state(self, connected):
        session, mock, device, events = connected
        await session.set_hold(True)
        assert b"!" in mock.sent()
        await events.wait_for(
            "state_changed", lambda p: p.status == DeviceStatus.HOLD
        )
        await session.set_hold(False)
        assert b"~" in mock.sent()

    async def test_resume_without_job_reports_idle(self, connected):
        session, mock, device, events = connected
        await session.set_hold(True)
        await session.set_hold(False)
        await events.wait_for(
            "state_changed", lambda p: p.status == DeviceStatus.IDLE
        )


class TestDeviceOperations:
    async def test_move_to_formatting(self, connected):
        session, mock, device, events = connected
        await session.move_to(1500, 10, -20.5)
        assert b"$J=G90 G21 F1500 X10.0 Y-20.5\n" in b"".join(mock.sent())

    async def test_jog_formatting(self, connected):
        session, mock, device, events = connected
        await session.jog(1000, [("x", 10), ("y", -2.5)])
        sent = b"".join(mock.sent())
        assert b"$J=G91 G21 F1000 X10.0 Y-2.5\n" in sent

    async def test_jog_no_axes_is_noop(self, connected):
        session, mock, device, events = connected
        mock.clear_sent()
        await session.jog(1000, [])
        assert b"".join(mock.sent()) == b""

    async def test_select_tool(self, connected):
        session, mock, device, events = connected
        await session.select_tool(2)
        assert b"T2\n" in b"".join(mock.sent())

    async def test_set_power_on(self, connected):
        session, mock, device, events = connected
        await session.set_power(500)
        assert b"M4 S500\n" in b"".join(mock.sent())

    async def test_set_power_off(self, connected):
        session, mock, device, events = connected
        await session.set_power(None)
        assert b"M5\n" in b"".join(mock.sent())

    async def test_set_focus_power(self, connected):
        session, mock, device, events = connected
        await session.set_focus_power(100)
        assert b"M3 S100\n" in b"".join(mock.sent())

    async def test_home_all(self, connected):
        session, mock, device, events = connected
        await session.home(None, "G54")
        text = b"".join(mock.sent())
        assert b"$H\n" in text
        assert b"G4 P0.01\n" in text
        assert b"G55\n" in text
        assert b"G54\n" in text
        assert text.index(b"$H\n") < text.index(b"G4 P0.01\n")
        assert text.index(b"G4 P0.01\n") < text.index(b"G55\n")
        assert text.index(b"G55\n") < text.index(b"G54\n")

    async def test_home_single_axis(self, connected):
        session, mock, device, events = connected
        await session.home(["X"], "G54")
        assert b"$HX\n" in b"".join(mock.sent())

    async def test_update_dialect(self, connected):
        session, mock, device, events = connected
        session.update_dialect({"move_to": "G0 X{x} Y{y}"})
        await session.move_to(1500, 10, 20)
        assert b"G0 X10.0 Y20.0\n" in b"".join(mock.sent())


class TestSettings:
    async def test_read_settings(self, connected):
        session, mock, device, events = connected
        pairs = await session.read_settings()
        as_dict = dict(pairs)
        assert as_dict["13"] == "0"
        assert as_dict["110"] == "500.000"

    async def test_write_setting(self, connected):
        session, mock, device, events = connected
        await session.write_setting("110", "750.000")
        assert b"$110=750.000\n" in b"".join(mock.sent())

    async def test_detect_unit_system_metric(self, connected):
        session, mock, device, events = connected
        assert await session.detect_unit_system() == "metric"

    async def test_detect_unit_system_imperial(self, connected):
        session, mock, device, events = connected
        device.settings = ["$13=1"]
        assert await session.detect_unit_system() == "imperial"


class TestWcs:
    async def test_read_wcs_offsets(self, connected):
        session, mock, device, events = connected
        offsets = await session.read_wcs_offsets()
        assert offsets["G54"] == (0.0, 0.0, 0.0)
        assert offsets["G55"] == (10.0, 20.0, 0.0)
        await events.wait_for("wcs_updated")

    async def test_read_wcs_offsets_imperial(self, connected):
        session, mock, device, events = connected
        device.settings = ["$13=1"]
        assert await session.detect_unit_system() == "imperial"
        offsets = await session.read_wcs_offsets()
        assert offsets["G55"][0] == pytest.approx(10.0 * 25.4)

    async def test_set_wcs_offset(self, connected):
        session, mock, device, events = connected
        await session.set_wcs_offset("G55", 10, 20, None)
        assert b"G10 L2 P2 X10.0 Y20.0\n" in b"".join(mock.sent())

    async def test_set_wcs_offset_with_z(self, connected):
        session, mock, device, events = connected
        await session.set_wcs_offset("G54", 1, 2, 3)
        assert b"G10 L2 P1 X1.0 Y2.0 Z3.0\n" in b"".join(mock.sent())

    async def test_set_wcs_offset_invalid_slot(self, connected):
        session, mock, device, events = connected
        with pytest.raises(RuntimeError):
            await session.set_wcs_offset("G53", 1, 2, None)

    async def test_read_parser_state(self, connected):
        session, mock, device, events = connected
        assert await session.read_parser_state() == "G54"


class TestProbe:
    async def test_successful_probe(self, connected):
        session, mock, device, events = connected
        pos = await session.run_probe_cycle("z", 10.0, 100)
        assert pos == (10.0, 20.0, 5.0)
        await events.wait_for(
            "probe_status_changed", lambda p: "triggered" in p
        )

    async def test_failed_probe(self, connected):
        session, mock, device, events = connected
        device.probe_response = "[PRB:0.0,0.0,0.0:0]"
        pos = await session.run_probe_cycle("Z", 10.0, 100)
        assert pos is None
        await events.wait_for(
            "probe_status_changed", lambda p: p == "Probe failed"
        )

    async def test_probe_unit_conversion(self, connected):
        session, mock, device, events = connected
        device.settings = ["$13=1"]
        await session.detect_unit_system()
        device.probe_response = "[PRB:1.0,1.0,1.0:1]"
        pos = await session.run_probe_cycle("Z", 10.0, 100)
        assert pos[0] == pytest.approx(25.4)


class TestAlarmLine:
    async def test_alarm_line_sets_state_error(self, connected):
        session, mock, device, events = connected
        mock.push(b"ALARM:1\r\n")
        payload = await events.wait_for(
            "state_changed", lambda p: p.error is not None
        )
        assert payload.error.code == 1
        assert payload.error.title == "Hard Limit"

    async def test_alarm_sets_error_via_status_report(self, connected):
        session, mock, device, events = connected
        mock.push(b"<Alarm:2|MPos:0,0,0>\r\n")
        payload = await events.wait_for(
            "state_changed", lambda p: p.error is not None
        )
        assert payload.error.title == "Soft Limit"
