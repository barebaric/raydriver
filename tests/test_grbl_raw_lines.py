"""Raw RX/TX line events surfaced to the Python event callback."""

from conftest import make_session, start_device

from raydriver.grbl import SessionEvent


class TestRawLines:
    async def test_raw_tx_reports_interactive_command(self, mock, events):
        emulator, device_task = await start_device(mock)
        session = make_session(mock, events)
        try:
            await session.connect()
            await events.wait_for(
                SessionEvent.ConnectionStatusChanged,
                lambda e: e.status == "CONNECTED",
            )
            events.events.clear()
            await session.execute_interactive_command("$$")
            await events.wait_for(SessionEvent.RawTx, lambda e: e.line == "$$")
        finally:
            await session.disconnect()
            device_task.cancel()

    async def test_raw_rx_reports_settings_reply(self, rig):
        session, mock, emulator, events = rig
        await session.execute_interactive_command("$$")
        await events.wait_for(SessionEvent.RawRx, lambda e: "ok" == e.line)
        assert any(
            isinstance(e, SessionEvent.RawRx) and e.line.startswith("$")
            for e in events.events
        )

    async def test_raw_rx_reports_status_report(self, rig):
        session, mock, emulator, events = rig
        event = await events.wait_for(
            SessionEvent.RawRx,
            lambda e: e.line.startswith("<") and e.line.endswith(">"),
        )
        line = event.line
        assert "Idle" in line or "Run" in line or "Hold" in line

    async def test_raw_rx_reports_welcome_message(self, mock, events):
        emulator, device_task = await start_device(mock)
        session = make_session(mock, events)
        try:
            await session.connect()
            await events.wait_for(
                SessionEvent.RawRx, lambda e: e.line.startswith("Grbl ")
            )
        finally:
            await session.disconnect()
            device_task.cancel()

    async def test_raw_lines_carry_full_conversation(self, rig):
        session, mock, emulator, events = rig
        await session.execute_interactive_command("$I")
        raw = [
            e.line
            for e in events.events
            if isinstance(e, (SessionEvent.RawRx, SessionEvent.RawTx))
        ]
        assert "$I" in raw
        assert any(line.startswith("[VER:") for line in raw)
        assert "ok" in raw
