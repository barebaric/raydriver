"""Stress tests: fragmented responses, byte-at-a-time acks, long
jobs with interleaved status reports (port of
test_grbl_serial_driver_stress.py)."""

import asyncio
import random

from conftest import STATUS_IDLE, make_session


def fragment(data, rng, min_size=1, max_size=12):
    chunks = []
    pos = 0
    while pos < len(data):
        size = rng.randint(min_size, max_size)
        chunks.append(data[pos : pos + size])
        pos += size
    return chunks


class ChunkedDevice:
    """Acks every command with fragmented `ok` responses, interleaved
    with status reports."""

    def __init__(self, mock, seed=42):
        self.mock = mock
        self.rng = random.Random(seed)
        self.status_every = 5
        self._seen = 0
        self._acks = 0
        self._welcome_sent = False
        self.silent = False

    async def run(self):
        while True:
            sent = self.mock.sent()
            if len(sent) < self._seen:
                self._seen = 0
            for chunk in sent[self._seen :]:
                await self._handle(chunk)
            self._seen = len(sent)
            await asyncio.sleep(0.004)

    async def _handle(self, chunk):
        if self.silent:
            return
        if not self._welcome_sent:
            self._welcome_sent = True
            for piece in fragment(b"Grbl 1.1h ['$' for help]\r\n", self.rng):
                self.mock.push(piece)
        if chunk == b"?":
            for piece in fragment(STATUS_IDLE, self.rng):
                self.mock.push(piece)
            return
        if chunk in (b"\x18", b"!", b"~"):
            return
        for _line in chunk.splitlines():
            if not _line.strip():
                continue
            self._acks += 1
            response = b"ok\r\n"
            if self._acks % self.status_every == 0:
                response = STATUS_IDLE + b"ok\r\n"
            await asyncio.sleep(0)
            for piece in fragment(response, self.rng):
                self.mock.push(piece)


class TestStress:
    async def test_fragmented_chunks_and_interleaved_reports(
        self, mock, events
    ):
        session = make_session(mock, events)
        device = ChunkedDevice(mock)
        device_task = asyncio.ensure_future(device.run())
        try:
            await session.connect()
            await events.wait_for(
                "connection_status_changed",
                lambda p: p[0] == "CONNECTED",
                timeout=5.0,
            )
            gcode = "\n".join(f"G1 X{i}" for i in range(200))
            await asyncio.wait_for(session.run(gcode), timeout=60.0)
            await asyncio.sleep(0.1)
            assert session.buffer_count == 0
            assert session.pending_commands == []
        finally:
            await session.disconnect()
            device_task.cancel()

    async def test_single_byte_acks(self, mock, events):
        session = make_session(mock, events)
        device = ChunkedDevice(mock)
        device_task = asyncio.ensure_future(device.run())
        try:
            await session.connect()
            await events.wait_for(
                "connection_status_changed",
                lambda p: p[0] == "CONNECTED",
                timeout=5.0,
            )
            gcode = "\n".join(f"G1 X{i}" for i in range(100))
            await asyncio.wait_for(session.run(gcode), timeout=60.0)
            await asyncio.sleep(0.1)
            assert session.buffer_count == 0
        finally:
            await session.disconnect()
            device_task.cancel()

    async def test_long_job_1000_lines(self, mock, events):
        session = make_session(mock, events)
        device = ChunkedDevice(mock, seed=7)
        device_task = asyncio.ensure_future(device.run())
        try:
            await session.connect()
            await events.wait_for(
                "connection_status_changed",
                lambda p: p[0] == "CONNECTED",
                timeout=5.0,
            )
            done = []
            gcode = "\n".join(f"G1 X{i}" for i in range(1000))
            op_map = {i: i for i in range(1000)}
            await asyncio.wait_for(
                session.run(gcode, op_map, [], done.append), timeout=120.0
            )
            await asyncio.sleep(0.1)
            assert session.buffer_count == 0
            assert session.pending_commands == []
            assert len(done) == 1000
        finally:
            await session.disconnect()
            device_task.cancel()
