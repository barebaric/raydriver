"""Raw RX/TX line events surfaced to the Python event callback."""

from conftest import make_session, start_device


class TestRawLines:
    async def test_raw_tx_reports_interactive_command(self, mock, events):
        emulator, device_task = await start_device(mock)
        session = make_session(mock, events)
        try:
            await session.connect()
            await events.wait_for(
                "connection_status_changed", lambda p: p[0] == "CONNECTED"
            )
            events.events.clear()
            await session.execute_interactive_command("$$")
            await events.wait_for("raw_tx", lambda line: line == "$$")
        finally:
            await session.disconnect()
            device_task.cancel()

    async def test_raw_rx_reports_settings_reply(self, rig):
        session, mock, emulator, events = rig
        await session.execute_interactive_command("$$")
        await events.wait_for("raw_rx", lambda line: "ok" == line)
        assert any(
            name == "raw_rx" and line.startswith("$")
            for name, line in events.events
        )

    async def test_raw_rx_reports_status_report(self, rig):
        session, mock, emulator, events = rig
        report = await events.wait_for(
            "raw_rx", lambda line: line.startswith("<") and line.endswith(">")
        )
        assert "Idle" in report or "Run" in report or "Hold" in report

    async def test_raw_rx_reports_welcome_message(self, mock, events):
        emulator, device_task = await start_device(mock)
        session = make_session(mock, events)
        try:
            await session.connect()
            await events.wait_for(
                "raw_rx", lambda line: line.startswith("Grbl ")
            )
        finally:
            await session.disconnect()
            device_task.cancel()

    async def test_raw_lines_carry_full_conversation(self, rig):
        session, mock, emulator, events = rig
        await session.execute_interactive_command("$I")
        raw = [
            line
            for name, line in events.events
            if name in ("raw_rx", "raw_tx")
        ]
        assert "$I" in raw
        assert any(line.startswith("[VER:") for line in raw)
        assert "ok" in raw
