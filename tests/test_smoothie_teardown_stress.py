"""Stress the Smoothie cancel/teardown path.

Reproduces the churn that preceded the intermittent Windows access
violation: many connect/run/cancel/disconnect/drop cycles with GC,
including dropping a session without an explicit disconnect so the
last session reference is released from a tokio worker. Guards the
teardown path against regressions; the runtime's GIL watchdog is
opt-in (RAYDRIVER_GIL_WATCHDOG) so it does not perturb these runs.
"""

import asyncio
import gc

import pytest

from raydriver.core.transport import MockTransport
from raydriver.smoothie import SmoothieSession
from raydriver.smoothie.emulator import SmoothieEmulator

GCODE = "".join(f"G1 X{i} F6000\n" for i in range(20))
TIMINGS = {
    "handshake_timeout": 2.0,
    "handshake_poll_interval": 0.02,
    "status_poll_interval": 60.0,
    "command_timeout": 2.0,
    "safety_shutdown_delay": 0.01,
}
DIALECT = {"safety_off_commands": ["M5"]}


async def _cancel_cycle(disconnect: bool) -> None:
    mock = MockTransport()
    emulator = SmoothieEmulator(mock)
    device_task = asyncio.ensure_future(emulator.run())
    session = SmoothieSession.with_transport(
        mock,
        config=TIMINGS,
        dialect=DIALECT,
        event_callback=lambda event: None,
    )
    try:
        await session.connect()
        for _ in range(100):
            if session.state.status.name != "UNKNOWN":
                break
            await asyncio.sleep(0.002)
        job = asyncio.ensure_future(session.run(GCODE))
        for _ in range(200):
            if len(mock.sent()) >= 2:
                break
            await asyncio.sleep(0.001)
        await session.cancel()
        try:
            await asyncio.wait_for(job, 3.0)
        except Exception:
            pass
        if disconnect:
            await session.disconnect()
    finally:
        device_task.cancel()
        try:
            await device_task
        except asyncio.CancelledError:
            pass
    del session
    gc.collect()


@pytest.mark.asyncio
async def test_cancel_disconnect_churn():
    for _ in range(60):
        await _cancel_cycle(disconnect=True)


@pytest.mark.asyncio
async def test_cancel_drop_without_disconnect_churn():
    for _ in range(60):
        await _cancel_cycle(disconnect=False)
