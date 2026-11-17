import asyncio
import ssl
from pathlib import Path

import pytest

from raydriver.xtool import XtoolM2Session
from raydriver.xtool.m2_emulator import XtoolM2Emulator


def test_session_exposes_m2_resource_uri_and_state():
    session = XtoolM2Session(config={"host": "192.0.2.20"})

    assert session.resource_uri == "wss://192.0.2.20:28900/websocket"
    assert session.state.status.name == "UNKNOWN"
    assert session.is_job_running is False


def test_session_has_control_operations_but_no_gcode_streamer():
    session = XtoolM2Session(config={"host": "192.0.2.20"})

    assert callable(session.start)
    assert callable(session.set_hold)
    assert callable(session.cancel)
    assert not hasattr(session, "run")


@pytest.mark.asyncio
async def test_operations_fail_cleanly_before_connecting():
    session = XtoolM2Session(config={"host": "192.0.2.20"})

    with pytest.raises(ConnectionError, match="not connected"):
        await session.home()


@pytest.mark.asyncio
async def test_session_connects_and_controls_m2_jobs():
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    certs = Path(__file__).parents[1] / "certs"
    context.load_cert_chain(
        certs / "analyzer-test-cert.pem",
        certs / "analyzer-test-key.pem",
    )
    emulator = XtoolM2Emulator(context)
    await emulator.start()
    session = XtoolM2Session(
        config={
            "host": "127.0.0.1",
            "port": emulator.port,
            "poll_interval": 5.0,
            "tls_allow_invalid_certs": True,
        }
    )
    try:
        await session.connect()
        await wait_until(lambda: session.state.status.name == "IDLE")
        await wait_until(lambda: session.state.machine_pos[0] == 1.0)

        await session.home()
        await session.move_to(10.0, 20.0)
        await session.jog(dx=1.0, dy=-1.0)
        await session.start()
        await wait_until(lambda: session.state.status.name == "RUN")
        await session.set_hold(True)
        await wait_until(lambda: session.state.status.name == "HOLD")
        await session.set_hold(False)
        await wait_until(lambda: session.state.status.name == "RUN")
        await session.cancel()
        await wait_until(lambda: session.state.status.name == "IDLE")

        actions = [
            request["params"]["action"]
            for request in emulator.requests
            if request["url"] == "/v1/project/device/control"
        ]
        assert actions == ["START", "PAUSE", "RESUME", "CANCEL"]
        urls = [request["url"] for request in emulator.requests]
        assert "/v1/user/parity" in urls
        assert "/v1/project/control/home" in urls
        assert "/v1/project/control/absolute-move" in urls
        assert "/v1/project/control/relative-move" in urls
    finally:
        await session.disconnect()
        await emulator.close()


async def wait_until(predicate, timeout=3.0):
    loop = asyncio.get_running_loop()
    deadline = loop.time() + timeout
    while loop.time() < deadline:
        if predicate():
            return
        await asyncio.sleep(0.01)
    raise AssertionError("condition was not met")
