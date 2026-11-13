"""Protocol-agnostic remote sessions: the daemon resolves the
protocol and exposes driver metadata as info; callers never name a
protocol."""

import asyncio

import pytest
import pytest_asyncio

from raydriver.core.transport import MockTransport
from raydriver.grbl import GrblSession
from raydriver.grbl.emulator import GrblEmulator
from raydriver.protocols import PROTOCOLS, protocol_info
from raydriver.remote import protocol
from raydriver.remote.client import list_devices, list_protocols, open_session
from raydriver.remote.daemon import Daemon, DaemonConfig

FAST_TIMINGS = {
    "handshake_timeout": 2.0,
    "handshake_poll_interval": 0.02,
    "status_poll_interval": 0.05,
    "command_timeout": 2.0,
    "poll_response_interval": 0.01,
    "poll_response_attempts": 10,
}


def test_registry_capabilities():
    grbl = protocol_info("grbl").capabilities
    assert grbl["supports_probing"] is True
    assert grbl["supports_settings"] is True
    assert grbl["reports_buffer_stats"] is True
    assert grbl["supports_clear_alarm"] is False
    smoothie = protocol_info("smoothie").capabilities
    assert smoothie["supports_probing"] is False
    assert smoothie["supports_clear_alarm"] is True
    assert smoothie["supports_hold"] is True
    marlin = protocol_info("marlin").capabilities
    assert marlin["supports_hold"] is False
    assert marlin["supports_unit_detection"] is True
    assert marlin["supports_clear_alarm"] is True
    assert (
        protocol_info("grbl_simple").capabilities["reports_buffer_stats"]
        is False
    )
    for info in PROTOCOLS.values():
        assert info.capabilities["uses_gcode"] is True
        assert info.dialect_family


@pytest_asyncio.fixture
async def rig():
    mock = MockTransport()
    emulator = GrblEmulator(mock, speed_factor=10)
    device_task = asyncio.ensure_future(emulator.run())

    def factory(name, config, dialect, cb):
        return GrblSession.with_transport(
            mock,
            config={**FAST_TIMINGS, **(config or {})},
            dialect=dialect or {},
            event_callback=cb,
        )

    daemon = Daemon(
        DaemonConfig(
            port=0,
            sessions=[
                {"protocol": "grbl", "config": {"port": "/dev/ttyUSB0"}}
            ],
        ),
        session_factory=factory,
    )
    server = await daemon.start()
    port = server.sockets[0].getsockname()[1]
    try:
        yield daemon, port
    finally:
        await daemon.stop()
        device_task.cancel()
        try:
            await device_task
        except asyncio.CancelledError:
            pass


async def test_protocol_list(rig):
    _, port = rig
    protocols = await list_protocols(
        {"remote_host": "127.0.0.1", "remote_port": port}
    )
    assert {p["name"] for p in protocols} == set(PROTOCOLS)
    grbl = next(p for p in protocols if p["name"] == "grbl")
    assert grbl["label"] == "GRBL"
    assert grbl["capabilities"]["supports_probing"] is True


async def test_device_list_includes_configured(rig):
    _, port = rig
    devices = await list_devices(
        {"remote_host": "127.0.0.1", "remote_port": port}
    )
    assert len(devices) == 1
    assert devices[0]["device"] == "/dev/ttyUSB0"
    assert devices[0]["protocol"] == "grbl"
    assert devices[0]["label"] == "GRBL"


async def test_open_by_device_resolves_protocol(rig):
    _, port = rig
    session = open_session(
        {"remote_host": "127.0.0.1", "remote_port": port},
        device="/dev/ttyUSB0",
        dialect={},
    )
    assert session.protocol is None
    try:
        await asyncio.wait_for(session.connect(), timeout=10)
        assert session.protocol == "grbl"
        assert session.metadata is not None
        assert session.metadata["dialect_family"] == "grbl"
        reply = await asyncio.wait_for(
            session.execute_interactive_command("$I"), timeout=10
        )
        assert any("VER:" in line for line in reply)
    finally:
        await session.disconnect()


async def test_disconnect_releases_control_lease(rig):
    # A probe-like client connects, then disconnects; a second client
    # (the machine) must then be able to take control of the device.
    _, port = rig
    endpoint = {"remote_host": "127.0.0.1", "remote_port": port}
    first = open_session(endpoint, device="/dev/ttyUSB0", dialect={})
    await asyncio.wait_for(first.connect(), timeout=10)
    await first.disconnect()

    second = open_session(endpoint, device="/dev/ttyUSB0", dialect={})
    try:
        await asyncio.wait_for(second.connect(), timeout=10)
        reply = await asyncio.wait_for(
            second.execute_interactive_command("$I"), timeout=10
        )
        assert any("VER:" in line for line in reply)
    finally:
        await second.disconnect()


async def test_open_by_url_endpoint(rig):
    _, port = rig
    session = open_session(
        f"raydriver://127.0.0.1:{port}", device="/dev/ttyUSB0", dialect={}
    )
    try:
        await asyncio.wait_for(session.connect(), timeout=10)
        assert session.protocol == "grbl"
    finally:
        await session.disconnect()


async def test_open_unknown_device():
    # A port-aware factory: only the known port opens, so probing an
    # unknown device fails and the daemon reports unknown_device.
    def factory(name, config, dialect, cb):
        if config.get("port") != "/dev/ttyUSB0":
            raise OSError("no such port")
        return GrblSession.with_transport(
            MockTransport(),
            config={**FAST_TIMINGS, **(config or {})},
            dialect=dialect or {},
            event_callback=cb,
        )

    daemon = Daemon(DaemonConfig(port=0), session_factory=factory)
    server = await daemon.start()
    port = server.sockets[0].getsockname()[1]
    try:
        session = open_session(
            {"remote_host": "127.0.0.1", "remote_port": port},
            device="/dev/nonexistent-raydriver",
            dialect={},
        )
        with pytest.raises(ConnectionError):
            await asyncio.wait_for(session.connect(), timeout=15)
    finally:
        await daemon.stop()


async def test_open_requires_device_or_session(rig):
    _, port = rig
    with pytest.raises(ValueError):
        open_session({"remote_host": "127.0.0.1", "remote_port": port})


async def test_per_protocol_method_validation(rig):
    daemon, port = rig
    # clear_alarm exists on Smoothie/Marlin, not GRBL: the daemon must
    # reject it for a grbl session even though it is in the union.
    host = daemon._resolve_device("/dev/ttyUSB0")[3]
    assert host is not None
    assert "clear_alarm" not in protocol_info(host.protocol_name).methods
    assert "clear_alarm" in protocol.SESSION_METHODS
