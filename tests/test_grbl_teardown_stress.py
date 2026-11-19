"""Stress the GRBL reconnect/teardown path.

The intermittent Windows access violation happened when a failed
reconnect attempt left its RX reader behind and the session was torn
down at the same time. Churn connect/disconnect cycles against a
silent transport so every reader must be reaped before the next
connection starts.
"""

import asyncio
import gc

import pytest
from conftest import DEFAULT_DIALECT

from raydriver.core.transport import MockTransport
from raydriver.grbl import GrblSession

TIMINGS = {
    "handshake_timeout": 0.05,
    "handshake_poll_interval": 0.01,
    "status_poll_interval": 60.0,
    "reconnect_delay": 0.01,
    "command_timeout": 1.0,
}


async def _reconnect_cycle(disconnect: bool) -> None:
    mock = MockTransport()
    session = GrblSession.with_transport(
        mock,
        config=TIMINGS,
        dialect=DEFAULT_DIALECT,
        event_callback=lambda event: None,
    )
    try:
        await session.connect()
        # Let the handshake fail so the connection loop reopens the
        # transport and spawns another reader.
        await asyncio.sleep(0.06)
    finally:
        if disconnect:
            await session.disconnect()
    del session
    gc.collect()


@pytest.mark.asyncio
async def test_reconnect_disconnect_churn():
    for _ in range(40):
        await _reconnect_cycle(disconnect=True)


@pytest.mark.asyncio
async def test_reconnect_drop_churn():
    for _ in range(40):
        await _reconnect_cycle(disconnect=False)
