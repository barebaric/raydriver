"""Transport handshake, Busy and reconnect tests for Ruida sessions."""

import asyncio
import sys

import pytest
import pytest_asyncio
from conftest import EventRecorder, wait_until

from raydriver.core import DriverEvent
from raydriver.ruida import RuidaDriver
from raydriver.ruida.emulator import RuidaEmulator, RuidaPtyEmulator

FAST = {
    "timeout": 0.2,
    "query_interval": 0.05,
    "query_retries": 2,
    "query_retry_delay": 0.05,
    "ping_retries": 2,
    "ping_delay": 0.05,
    "reconnect_delay": 0.1,
}


def make_session(events, emulator, protocol="tcp", **config):
    return RuidaDriver(
        config={
            "host": "127.0.0.1",
            "port": emulator.port,
            "protocol": protocol,
            **FAST,
            **config,
        },
        event_callback=events.callback,
    )


async def connect(session, events):
    await session.connect()
    await events.wait_for(
        DriverEvent.ConnectionStatusChanged,
        lambda event: event.status == "CONNECTED",
    )
    await wait_until(lambda: session.state.status.name == "IDLE")


@pytest_asyncio.fixture(params=["tcp", "udp"])
async def protocol_emulator(request):
    emulator = RuidaEmulator(protocol=request.param)
    await emulator.start()
    try:
        yield request.param, emulator
    finally:
        await emulator.stop()


@pytest.fixture
def events():
    return EventRecorder()


def connection_statuses(events):
    return [
        event.status
        for event in events.events
        if isinstance(event, DriverEvent.ConnectionStatusChanged)
    ]


class TestPacketHandshake:
    @pytest.mark.asyncio
    async def test_connects_and_queries_over_tcp_and_udp(
        self, protocol_emulator, events
    ):
        protocol, emulator = protocol_emulator
        session = make_session(events, emulator, protocol=protocol)
        try:
            await connect(session, events)
            await wait_until(
                lambda: session.machine_status["card_id"] == emulator.card_id
            )
            assert session.state.status.name != "UNKNOWN"
            assert session.resource_uri == (
                f"{protocol}://127.0.0.1:{emulator.port}"
            )
            assert emulator.commands
        finally:
            await session.disconnect()

    @pytest.mark.asyncio
    async def test_busy_query_keeps_the_connection(
        self, protocol_emulator, events
    ):
        protocol, emulator = protocol_emulator
        session = make_session(events, emulator, protocol=protocol)
        try:
            await connect(session, events)
            emulator.withhold_reply = True
            await asyncio.sleep(FAST["timeout"] * 2)
            assert session.state.status.name != "UNKNOWN"
            assert "ERROR" not in connection_statuses(events)
            await wait_until(
                lambda: session.machine_status["card_id"] is not None
            )
        finally:
            await session.disconnect()

    @pytest.mark.asyncio
    async def test_missing_ack_is_retried(self, protocol_emulator, events):
        protocol, emulator = protocol_emulator
        session = make_session(events, emulator, protocol=protocol)
        try:
            await connect(session, events)
            emulator.fail_next_ack = True
            await asyncio.sleep(FAST["timeout"] * 2)
            assert session.state.status.name != "UNKNOWN"
        finally:
            await session.disconnect()


class TestReconnect:
    @pytest.mark.asyncio
    async def test_reconnects_after_the_link_drops(self, events):
        emulator = RuidaEmulator(protocol="tcp")
        await emulator.start()
        session = make_session(events, emulator, protocol="tcp")
        try:
            await connect(session, events)
            emulator.disconnect_clients()
            await events.wait_for(
                DriverEvent.ConnectionStatusChanged,
                lambda event: event.status == "SLEEPING",
            )
            await events.wait_for(
                DriverEvent.ConnectionStatusChanged,
                lambda event: event.status == "CONNECTED",
            )
            assert session.state.status.name != "UNKNOWN"
        finally:
            await session.disconnect()
            await emulator.stop()


@pytest.mark.skipif(
    sys.platform == "win32", reason="PTY USB emulation is POSIX-only"
)
class TestInterfaceSwitch:
    @pytest.mark.asyncio
    async def test_moves_to_network_and_back_to_usb(self, events):
        network = RuidaEmulator(protocol="tcp", card_id=0x22222222)
        usb = RuidaPtyEmulator(card_id=0x11111111)
        await network.start()
        session = RuidaDriver(
            config={
                "host": "127.0.0.1",
                "port": network.port,
                "protocol": "tcp",
                "usb_device": usb.path,
                **FAST,
            },
            event_callback=events.callback,
        )
        try:
            await session.connect()
            await wait_until(
                lambda: (session.resource_uri or "").startswith("serial://")
            )
            await wait_until(
                lambda: (
                    session.machine_status["card_id"] == usb.controller.card_id
                )
            )

            usb.unplug()
            await wait_until(
                lambda: (session.resource_uri or "").startswith("tcp://"),
                timeout=8.0,
            )
            await wait_until(
                lambda: session.machine_status["card_id"] == network.card_id
            )

            usb.plug()
            network.disconnect_clients()
            await wait_until(
                lambda: (session.resource_uri or "").startswith("serial://"),
                timeout=8.0,
            )
            await wait_until(
                lambda: (
                    session.machine_status["card_id"] == usb.controller.card_id
                )
            )
        finally:
            await session.disconnect()
            await network.stop()
            usb.stop()
