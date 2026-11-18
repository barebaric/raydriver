"""Device census tests: ``describe()`` parity fixtures and
``probe_device()`` against the GRBL firmware emulator, locally and
through the remote daemon."""

import asyncio

import pytest
from conftest import (
    DEFAULT_DIALECT,
    FAST_TIMINGS,
    EventRecorder,
    server_port,
)

from raydriver.core import SessionEvent
from raydriver.core.transport import MockTransport
from raydriver.grbl import GrblSession
from raydriver.grbl.emulator import GrblEmulator
from raydriver.grbl.probe import (
    GrblDeviceInfo,
    GrblFlavor,
    describe,
    probe_device,
)
from raydriver.remote.client import create_session
from raydriver.remote.daemon import Daemon, DaemonConfig


class TestDescribe:
    def test_full_census(self):
        build_info = [
            "[VER:1.1h.ORTUR:]",
            "[OPT:VMPH,63,511]",
        ]
        settings_lines = [
            "$110=3000.0",
            "$111=3000.0",
            "$120=500.0",
            "$121=500.0",
            "$130=400.0",
            "$131=300.0",
            "$22=1.0",
            "$30=1000.0",
            "$32=1.0",
            "$13=0.0",
            "$12=0.002",
        ]
        info = describe(build_info, settings_lines)
        assert info.name == "ORTUR"
        assert info.firmware_version == "1.1h"
        assert info.rx_buffer_size == 511
        assert info.compile_flags == "VMPH"
        assert info.flavor is GrblFlavor.STOCK
        assert info.max_travel == (400.0, 300.0)
        assert info.max_speed == 3000
        assert info.acceleration == 500
        assert info.max_spindle == 1000
        assert info.laser_mode is True
        assert info.homing_enabled is True
        assert info.report_inches is False
        assert info.arc_tolerance == 0.002
        assert info.single_axis_homing is True
        assert info.settings["110"] == 3000.0

    def test_minimal_census(self):
        info = describe([], [])
        assert info.name == "Unknown Grbl Device"
        assert info.firmware_version is None
        assert info.rx_buffer_size is None
        assert info.compile_flags == ""
        assert info.flavor is GrblFlavor.UNKNOWN
        assert info.settings == {}
        assert info.max_travel is None
        assert info.max_speed is None
        assert info.acceleration is None
        assert info.max_spindle is None
        assert info.laser_mode is None
        assert info.homing_enabled is None
        assert info.report_inches is None
        assert info.arc_tolerance is None
        assert info.single_axis_homing is False

    def test_different_speeds_per_axis(self):
        info = describe(
            [],
            [
                "$110=5000.0",
                "$111=3000.0",
                "$120=800.0",
                "$121=600.0",
            ],
        )
        assert info.max_speed == 3000
        assert info.acceleration == 600

    def test_flavor_stock(self):
        build_info = ["[VER:1.1h:]", "[OPT:VMPH,63,511]"]
        assert describe(build_info, []).flavor is GrblFlavor.STOCK

    def test_flavor_stock_with_build_name(self):
        build_info = ["[VER:1.1h.ORTUR:]", "[OPT:VMPH,63,511]"]
        assert describe(build_info, []).flavor is GrblFlavor.STOCK

    def test_flavor_stock_with_comma_format(self):
        build_info = ["[VER:1.0.15,20240923:]", "[OPT:VMP,31,511]"]
        assert describe(build_info, []).flavor is GrblFlavor.STOCK

    def test_flavor_grblhal_in_ver_line(self):
        build_info = ["[VER:grblHAL 1.1f:]", "[OPT:VMP,31,511]"]
        assert describe(build_info, []).flavor is GrblFlavor.GRBLHAL

    def test_flavor_fluidnc_in_ver_line(self):
        build_info = ["[VER:FluidNC 3.6.1:]", "[OPT:VMP,31,511]"]
        assert describe(build_info, []).flavor is GrblFlavor.FLUIDNC

    def test_flavor_grblhal_settings_keys(self):
        build_info = ["[VER:1.1f:]"]
        settings = ["$0=10", "$400=200.0", "$401=300.0"]
        assert describe(build_info, settings).flavor is GrblFlavor.GRBLHAL

    def test_flavor_unknown(self):
        assert describe(["garbage"], []).flavor is GrblFlavor.UNKNOWN


class TestProbeDevice:
    async def test_probe_device_against_emulator(self, rig):
        session, mock, emulator, events = rig
        info = await probe_device(session)
        assert isinstance(info, GrblDeviceInfo)
        assert info.name == "EMULATOR"
        assert info.firmware_version == "1.1h"
        assert info.rx_buffer_size == 128
        assert info.compile_flags == "V"
        assert info.flavor is GrblFlavor.STOCK
        assert info.max_travel == (400.0, 300.0)
        assert info.max_speed == 500
        assert info.acceleration == 10
        assert info.max_spindle == 1000
        assert info.laser_mode is False
        assert info.homing_enabled is False
        assert info.report_inches is False
        assert info.arc_tolerance == pytest.approx(0.002)
        assert info.single_axis_homing is False


@pytest.fixture
async def remote_rig():
    """A daemon hosting one emulator-backed grbl session."""
    mock = MockTransport()
    emulator = GrblEmulator(mock, speed_factor=10)
    device_task = asyncio.ensure_future(emulator.run())

    def factory(protocol_name, config, dialect, event_callback):
        assert protocol_name == "grbl"
        return GrblSession.with_transport(
            mock,
            config={**FAST_TIMINGS, **(config or {})},
            dialect={**DEFAULT_DIALECT, **(dialect or {})},
            event_callback=event_callback,
        )

    daemon = Daemon(
        DaemonConfig(port=0, keepalive_interval=0.5, lease_grace=0.5),
        session_factory=factory,
    )
    server = await daemon.start()
    try:
        yield server_port(server)
    finally:
        await daemon.stop()
        device_task.cancel()
        try:
            await device_task
        except asyncio.CancelledError:
            pass


class TestProbeDeviceRemote:
    async def test_probe_device_over_remote(self, remote_rig):
        port = remote_rig
        events = EventRecorder()
        session = create_session(
            "grbl",
            {"remote_host": "127.0.0.1", "remote_port": port},
            None,
            events.callback,
        )
        try:
            await session.connect()
            await events.wait_for(
                SessionEvent.ConnectionStatusChanged,
                lambda event: event.status == "CONNECTED",
            )
            info = await probe_device(session)
        finally:
            await session.disconnect()
        assert info.name == "EMULATOR"
        assert info.firmware_version == "1.1h"
        assert info.rx_buffer_size == 128
        assert info.flavor is GrblFlavor.STOCK
        assert info.max_speed == 500
        assert info.acceleration == 10
