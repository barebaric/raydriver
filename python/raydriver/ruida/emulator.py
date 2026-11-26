"""A Ruida controller emulator (not a mock) for tests.

Speaks the controller's binary protocol over TCP or UDP: it
acknowledges every packet, answers ``GET_SETTING`` queries with typed
replies and tracks the job state commanded by ``START_JOB``/
``STOP_JOB``/``PAUSE_JOB``/``RESUME_JOB``/``END_JOB``. Tests inspect
``commands`` (the unswizzled command bytes, as hex) and drive the
reported state through ``set_job_running``, ``set_paused`` and
``set_position``.

Fault controls:

- ``fail_next_ack``: drop the next packet without acknowledging it.
- ``withhold_reply``: acknowledge the next query but send no reply,
  exercising the transport's Busy path.
- ``disconnect_clients()``: close active TCP sessions to force a
  reconnect.

Used by the test suite for hardware-free session coverage.
"""

from __future__ import annotations

import asyncio

ACK = 0xCC
CARD_ID_ADDRESS = 0x057E
MACHINE_STATUS_ADDRESS = 0x0400
POSITION_ADDRESSES = (0x0421, 0x0431, 0x0441, 0x0451)
BED_SIZE_ADDRESSES = (0x0026, 0x0036)
MAINBOARD_VERSION_ADDRESS = 0x057F
MACHINE_FEATURES_ADDRESS = 0x030F

STATUS_RUNNING = 0x00000001
STATUS_PAUSED = 0x00000002
STATUS_MOVING = 0x01000000


def swizzle_byte(byte: int, magic: int) -> int:
    byte ^= byte >> 7
    byte ^= (byte << 7) & 0xFF
    byte ^= byte >> 7
    byte ^= magic
    return (byte + 1) & 0xFF


def unswizzle_byte(byte: int, magic: int) -> int:
    byte = (byte - 1) & 0xFF
    byte ^= magic
    byte ^= byte >> 7
    byte ^= (byte << 7) & 0xFF
    byte ^= byte >> 7
    return byte


def uint35(value: int) -> bytes:
    return bytes((value >> shift) & 0x7F for shift in range(28, -1, -7))


def int35(value: int) -> bytes:
    negative = value < 0
    if negative:
        value = (1 << 34) + value
    data = bytearray(uint35(value))
    if negative:
        data[0] |= 0x40
    return bytes(data)


class RuidaEmulator:
    """A firmware-faithful Ruida controller for tests."""

    def __init__(
        self,
        magic: int = 0x88,
        card_id: int = 0x65106510,
        protocol: str = "tcp",
    ):
        if protocol not in ("tcp", "udp"):
            raise ValueError(f"unsupported protocol: {protocol}")
        self.protocol = protocol
        self.magic = magic
        self.card_id = card_id
        self.mainboard_version = "RDC6442S"
        self.features = 0x0001
        self.bed_size = [400.0, 400.0]
        self.position = [0.0, 0.0, 0.0, 0.0]
        self.status = 0
        self.commands: list[str] = []
        self.fail_next_ack = False
        self.withhold_reply = False
        self.reply_chunk_size: int | None = None
        self.port: int | None = None
        self._server: asyncio.AbstractServer | None = None
        self._udp_transport: asyncio.DatagramTransport | None = None
        self._clients: set = set()

    async def start(self) -> None:
        if self.protocol == "udp":
            loop = asyncio.get_running_loop()
            transport, _ = await loop.create_datagram_endpoint(
                lambda: _UdpProtocol(self),
                local_addr=("127.0.0.1", 0),
            )
            self._udp_transport = transport
            self.port = transport.get_extra_info("sockname")[1]
            return
        self._server = await asyncio.start_server(self._handle, "127.0.0.1", 0)
        self.port = self._server.sockets[0].getsockname()[1]

    async def stop(self) -> None:
        if self._server is not None:
            self._server.close()
            await self._server.wait_closed()
        if self._udp_transport is not None:
            self._udp_transport.close()
        self.disconnect_clients()

    def disconnect_clients(self) -> None:
        """Closes active TCP sessions; harmless for UDP."""
        for writer in list(self._clients):
            writer.close()

    # -- test-facing state ------------------------------------------------

    @property
    def job_running(self) -> bool:
        return bool(self.status & STATUS_RUNNING)

    @property
    def paused(self) -> bool:
        return bool(self.status & STATUS_PAUSED)

    def set_job_running(self, running: bool) -> None:
        if running:
            self.status |= STATUS_RUNNING
        else:
            self.status &= ~STATUS_RUNNING

    def set_paused(self, paused: bool) -> None:
        if paused:
            self.status |= STATUS_PAUSED
        else:
            self.status &= ~STATUS_PAUSED

    def set_moving(self, moving: bool) -> None:
        if moving:
            self.status |= STATUS_MOVING
        else:
            self.status &= ~STATUS_MOVING

    def set_position(self, x: float, y: float, z: float = 0.0) -> None:
        self.position = [x, y, z, 0.0]

    # -- protocol ----------------------------------------------------------

    def _swizzle(self, data: bytes) -> bytes:
        return bytes(swizzle_byte(byte, self.magic) for byte in data)

    def _unswizzle(self, data: bytes) -> bytes:
        return bytes(unswizzle_byte(byte, self.magic) for byte in data)

    def _value_for(self, address: int) -> bytes:
        if address == CARD_ID_ADDRESS:
            return uint35(self.card_id)
        if address == MACHINE_STATUS_ADDRESS:
            return uint35(self.status)
        if address in POSITION_ADDRESSES:
            index = POSITION_ADDRESSES.index(address)
            return int35(round(self.position[index] * 1000))
        if address in BED_SIZE_ADDRESSES:
            index = BED_SIZE_ADDRESSES.index(address)
            return int35(round(self.bed_size[index] * 1000))
        if address == MAINBOARD_VERSION_ADDRESS:
            return self.mainboard_version.encode() + b"\x00"
        if address == MACHINE_FEATURES_ADDRESS:
            return uint35(self.features)
        return uint35(0)

    def _reply(self, address: int, data: bytes) -> bytes:
        return self._swizzle(
            bytes(
                [
                    0xDA,
                    0x01,
                    (address >> 8) & 0x7F,
                    address & 0x7F,
                ]
            )
            + data
        )

    def _apply_commands(self, raw: bytes) -> None:
        for index in range(len(raw) - 1):
            prefix, opcode = raw[index], raw[index + 1]
            if prefix == 0xD8 and opcode == 0x00:
                self.set_job_running(True)
                self.set_paused(False)
            elif prefix == 0xD8 and opcode == 0x01:
                self.set_job_running(False)
                self.set_paused(False)
            elif prefix == 0xD8 and opcode == 0x02:
                self.set_paused(True)
            elif prefix == 0xD8 and opcode == 0x03:
                self.set_paused(False)
            elif prefix == 0xE5 and opcode == 0x05:
                self.set_job_running(False)
                self.set_paused(False)

    async def _handle(self, reader, writer):
        self._clients.add(writer)
        try:
            while True:
                data = await reader.read(4096)
                if not data:
                    break
                await self._process(data, writer)
        except (ConnectionError, asyncio.IncompleteReadError):
            pass
        finally:
            self._clients.discard(writer)
            writer.close()

    async def _process(self, data: bytes, writer) -> None:
        ack, replies = self._handle_packet(data)
        if ack is None:
            return
        writer.write(ack)
        await writer.drain()
        if replies:
            if self.reply_chunk_size:
                for index in range(0, len(replies), self.reply_chunk_size):
                    writer.write(
                        replies[index : index + self.reply_chunk_size]
                    )
                    await writer.drain()
                    await asyncio.sleep(0.005)
            else:
                writer.write(replies)
                await writer.drain()

    def _handle_packet(self, data: bytes) -> tuple[bytes | None, bytes]:
        """Applies one raw command packet and returns (ACK, replies)."""
        raw = self._unswizzle(data)
        self.commands.append(raw.hex())
        self._apply_commands(raw)
        if self.fail_next_ack:
            self.fail_next_ack = False
            return None, b""
        ack = self._swizzle(bytes([ACK]))
        if self.withhold_reply:
            self.withhold_reply = False
            return ack, b""
        index = 0
        replies = bytearray()
        while index + 3 < len(raw):
            if raw[index] == 0xDA and raw[index + 1] == 0x00:
                address = (raw[index + 2] << 8) | raw[index + 3]
                replies.extend(self._reply(address, self._value_for(address)))
                index += 4
            else:
                index += 1
        return ack, bytes(replies)


class _UdpProtocol(asyncio.DatagramProtocol):
    """Routes UDP datagrams through the same controller logic as TCP."""

    def __init__(self, emulator: RuidaEmulator):
        self.emulator = emulator
        self.transport: asyncio.DatagramTransport | None = None
        self.addr = None

    def connection_made(self, transport) -> None:
        self.transport = transport

    def datagram_received(self, data: bytes, addr) -> None:
        if len(data) < 2 or self.transport is None:
            return
        checksum = (data[0] << 8) | data[1]
        payload = data[2:]
        if sum(payload) & 0xFFFF != checksum:
            return
        ack, replies = self.emulator._handle_packet(payload)
        if ack is None:
            return
        self.transport.sendto(ack, addr)
        if replies:
            self.transport.sendto(replies, addr)


class RuidaPtyEmulator:
    """A USB Ruida controller on a pseudo-terminal (POSIX tests only).

    USB carries raw swizzled bytes and no per-packet ACK. ``path`` is a
    stable symlink that can be removed to simulate an unplugged cable
    and recreated after ``plug()``; the driver keeps using the same
    configured device path.
    """

    def __init__(self, magic: int = 0x88, card_id: int = 0x65106510):
        import sys

        if sys.platform == "win32":
            raise RuntimeError("PTY emulation is not available on Windows")
        self.controller = RuidaEmulator(magic=magic, card_id=card_id)
        self._master_fd = -1
        self._slave_fd = -1
        self._directory = None
        self.path = ""
        self._loop = None
        self._plug()

    def _plug(self) -> None:
        import os
        import pty
        import tempfile
        import tty

        if self._directory is None:
            self._directory = tempfile.mkdtemp(prefix="raydriver-ruida-")
            self.path = os.path.join(self._directory, "ttyUSB0")
        master, slave = pty.openpty()
        tty.setraw(slave)
        os.set_blocking(master, False)
        if os.path.lexists(self.path):
            os.unlink(self.path)
        os.symlink(os.ttyname(slave), self.path)
        self._master_fd = master
        self._slave_fd = slave
        self._loop = asyncio.get_running_loop()
        self._loop.add_reader(master, self._on_readable)

    def unplug(self) -> None:
        """Removes the device path and closes the PTY."""
        import os

        if self._loop is not None and self._master_fd >= 0:
            self._loop.remove_reader(self._master_fd)
        if os.path.lexists(self.path):
            os.unlink(self.path)
        for fd in (self._master_fd, self._slave_fd):
            if fd >= 0:
                try:
                    os.close(fd)
                except OSError:
                    pass
        self._master_fd = -1
        self._slave_fd = -1

    def plug(self) -> None:
        """Recreates the device path with a fresh PTY."""
        self._plug()

    def stop(self) -> None:
        import shutil

        self.unplug()
        if self._directory:
            shutil.rmtree(self._directory, ignore_errors=True)
            self._directory = None

    def _on_readable(self) -> None:
        import os

        while True:
            try:
                data = os.read(self._master_fd, 4096)
            except BlockingIOError:
                return
            except OSError:
                return
            if not data:
                return
            _, replies = self.controller._handle_packet(data)
            if replies:
                os.write(self._master_fd, replies)
