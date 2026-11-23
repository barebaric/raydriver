"""Epilog session tests, driven against an in-process fake LPD
printer.

The fake printer speaks the BSD "receive job" exchange the machine
expects, so the tests assert the exact wire bytes: the five stages,
their single-byte acks, one connection per job and no retry of a
partial transfer.
"""

import asyncio
from contextlib import asynccontextmanager

import pytest
from conftest import EventRecorder

from raydriver.core import SessionEvent
from raydriver.epilog import EpilogSession


@asynccontextmanager
async def printer(handler):
    """Run *handler(reader, writer)* for every accepted connection."""
    tasks = set()

    async def connection(reader, writer):
        task = asyncio.current_task()
        tasks.add(task)
        try:
            await handler(reader, writer)
        finally:
            writer.close()
            await writer.wait_closed()
            tasks.discard(task)

    server = await asyncio.start_server(connection, "127.0.0.1", 0)
    port = server.sockets[0].getsockname()[1]
    try:
        yield port
    finally:
        server.close()
        await server.wait_closed()
        for task in list(tasks):
            task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)


def make_session(port, events, **config):
    return EpilogSession(
        config={"host": "127.0.0.1", "port": port, "timeout": 2.0, **config},
        event_callback=events.callback,
    )


async def test_upload_performs_the_lpd_exchange():
    received = asyncio.get_running_loop().create_future()
    payload = b"PJL-test-payload" + bytes(4096)

    async def receive(reader, writer):
        try:
            assert await reader.readline() == b"\x02\n"
            writer.write(b"\x00")
            control_header = await reader.readline()
            assert control_header.startswith(b"\x02")
            size, control_name = control_header[1:].strip().split(b" ")
            assert control_name.startswith(b"cfA")
            writer.write(b"\x00")
            control = await reader.readexactly(int(size))
            assert await reader.readexactly(1) == b"\x00"
            writer.write(b"\x00")
            data_header = await reader.readline()
            assert data_header.startswith(b"\x03")
            size, data_name = data_header[1:].strip().split(b" ")
            assert data_name == b"df" + control_name[2:]
            assert int(size) == len(payload)
            writer.write(b"\x00")
            assert await reader.readexactly(int(size)) == payload
            assert b"l" + data_name + b"\n" in control
            assert b"U" + data_name + b"\n" in control
            assert b"JJob___E\n" in control
            writer.write(b"\x00")
            await writer.drain()
            assert await reader.read() == b""
            received.set_result(True)
        except (AssertionError, OSError, ValueError) as exc:
            received.set_exception(exc)

    events = EventRecorder()
    async with printer(receive) as port:
        session = make_session(port, events)
        await session.upload(payload, "Job\r\n\x1bE")
        assert await asyncio.wait_for(received, 2)
        assert "JobFinished" in events.names()
        await session.disconnect()


@pytest.mark.parametrize("rejected_stage", range(5))
async def test_rejection_closes_without_retry(rejected_stage):
    connections = 0
    closed = asyncio.Event()

    async def reject(reader, writer):
        nonlocal connections
        connections += 1
        size = 0
        for stage in range(5):
            if stage in (0, 1, 3):
                line = await reader.readline()
                if stage in (1, 3):
                    size = int(line[1:].split(b" ")[0])
            else:
                await reader.readexactly(size + (1 if stage == 2 else 0))
            writer.write(b"\x01" if stage == rejected_stage else b"\x00")
            await writer.drain()
            if stage == rejected_stage:
                assert await reader.read() == b""
                closed.set()
                return

    events = EventRecorder()
    async with printer(reject) as port:
        session = make_session(port, events)
        with pytest.raises(RuntimeError, match="rejected"):
            await session.upload(b"payload", "test")
        await asyncio.wait_for(closed.wait(), 2)
        await session.disconnect()
    assert connections == 1


async def test_missing_ack_times_out_and_closes():
    closed = asyncio.Event()

    async def silent(reader, writer):
        await reader.readline()
        assert await reader.read() == b""
        closed.set()

    events = EventRecorder()
    async with printer(silent) as port:
        session = make_session(port, events, timeout=0.05)
        with pytest.raises(TimeoutError, match="timed out"):
            await session.upload(b"test", "test")
        await asyncio.wait_for(closed.wait(), 2)
        await session.disconnect()


async def test_disconnect_is_not_success():
    async def disconnect(reader, writer):
        await reader.readline()

    events = EventRecorder()
    async with printer(disconnect) as port:
        session = make_session(port, events)
        with pytest.raises(ConnectionError, match="disconnected"):
            await session.upload(b"test", "test")
        await session.disconnect()


async def test_cancel_closes_connection():
    waiting = asyncio.Event()
    closed = asyncio.Event()

    async def silent(reader, writer):
        await reader.readline()
        waiting.set()
        assert await reader.read() == b""
        closed.set()

    events = EventRecorder()
    async with printer(silent) as port:
        session = make_session(port, events)
        task = asyncio.ensure_future(session.upload(b"test", "test"))
        await asyncio.wait_for(waiting.wait(), 2)
        await session.cancel()
        with pytest.raises(Exception):
            await asyncio.wait_for(task, 2)
        await asyncio.wait_for(closed.wait(), 2)
        await session.disconnect()


async def test_probe_sends_nothing():
    received = asyncio.get_running_loop().create_future()

    async def record(reader, writer):
        received.set_result(await reader.read())

    events = EventRecorder()
    async with printer(record) as port:
        session = make_session(port, events)
        await session.probe()
        assert await asyncio.wait_for(received, 2) == b""
        await session.disconnect()


async def test_connect_reports_reachable_machine():
    async def idle(reader, writer):
        await reader.read()

    events = EventRecorder()
    async with printer(idle) as port:
        session = make_session(port, events)
        await session.connect()
        event = await events.wait_for(
            SessionEvent.ConnectionStatusChanged,
            lambda event: event.status == "CONNECTED",
        )
        assert "reachable" in (event.message or "")
        await session.disconnect()


async def test_connect_failure_reports_error():
    # Grab a free port, then close it again so nothing listens.
    server = await asyncio.start_server(lambda r, w: None, "127.0.0.1", 0)
    port = server.sockets[0].getsockname()[1]
    server.close()
    await server.wait_closed()

    events = EventRecorder()
    session = make_session(port, events)
    with pytest.raises(ConnectionError):
        await session.connect()
    await events.wait_for(
        SessionEvent.ConnectionStatusChanged,
        lambda event: event.status == "ERROR",
    )
    await session.disconnect()


async def test_empty_payload_is_rejected():
    events = EventRecorder()
    async with printer(lambda r, w: r.read()) as port:
        session = make_session(port, events)
        with pytest.raises(RuntimeError, match="empty"):
            await session.upload(b"", "test")
        await session.disconnect()


async def test_gcode_run_is_unsupported():
    events = EventRecorder()
    async with printer(lambda r, w: r.read()) as port:
        session = make_session(port, events)
        with pytest.raises(NotImplementedError):
            await session.run("G1 X0")
        await session.disconnect()


async def test_machine_space_and_single_wcs():
    events = EventRecorder()
    async with printer(lambda r, w: r.read()) as port:
        session = make_session(port, events)
        assert await session.read_parser_state() == "MACHINE"
        assert await session.read_wcs_offsets() == {"MACHINE": (0.0, 0.0, 0.0)}
        await session.disconnect()


def test_registry_entry():
    from raydriver.protocols import protocol_info

    info = protocol_info("epilog")
    assert info.label == "Epilog"
    assert info.connection == "network"
    assert "upload" in info.methods
    assert info.capabilities["uses_gcode"] is False
