"""Shared fixtures: a fake GRBL device driving a MockTransport."""

import asyncio
import time

import pytest

from raydriver.grbl import GrblSession, MockTransport

WELCOME = b"Grbl 1.1h ['$' for help]\r\n"
STATUS_IDLE = b"<Idle|MPos:0.000,0.000,0.000|Bf:15,127>\r\n"
STATUS_RUN = b"<Run|MPos:1.000,2.000,0.000|FS:500,0>\r\n"

DEFAULT_SETTINGS = [
    "$0=10",
    "$13=0",
    "$22=1",
    "$30=1000",
    "$32=1",
    "$110=500.000",
    "$111=500.000",
    "$130=300.000",
    "$131=300.000",
]

DEFAULT_WCS_LINES = [
    "[G54:0.000,0.000,0.000]",
    "[G55:10.000,20.000,0.000]",
]

FAST_TIMINGS = {
    "handshake_timeout": 2.0,
    "handshake_poll_interval": 0.02,
    "status_poll_interval": 0.05,
    "reconnect_delay": 0.2,
    "command_timeout": 2.0,
    "poll_response_interval": 0.01,
    "poll_response_attempts": 10,
    "safety_shutdown_delay": 0.02,
    "stall_timeout_default": 0.3,
    "stall_timeout_min": 0.2,
    "stall_timeout_max": 1.0,
}

DEFAULT_DIALECT = {
    "home_all": "$H",
    "home_axis": "$H{axis_letter}",
    "move_to": "$J=G90 G21 F{speed} X{x} Y{y}",
    "jog": "$J=G91 G21 F{speed}",
    "clear_alarm": "$X",
    "laser_on": "M4 S{power:.0f}",
    "laser_off": "M5",
    "focus_laser_on": "M3 S{power:.0f}",
    "tool_change": "T{tool_number}",
    "set_wcs_offset": "G10 L2 P{p_num} X{x} Y{y} Z{z}",
    "probe_cycle": "G38.2 {axis_letter}{max_travel} F{feed_rate}",
    "safety_off_commands": ["M5", "M9"],
}


class EventRecorder:
    def __init__(self):
        self.events = []

    def callback(self, name, payload=None):
        self.events.append((name, payload))

    def names(self):
        return [name for name, _ in self.events]

    async def wait_for(self, name, pred=None, timeout=5.0):
        """Wait until an event *name* (matching *pred*) was recorded."""
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            for event_name, payload in self.events:
                if event_name != name:
                    continue
                if pred is None or pred(payload):
                    return payload
            await asyncio.sleep(0.01)
        raise AssertionError(
            f"event {name!r} not observed within {timeout:.1f}s; "
            f"got {self.names()}"
        )


class FakeDevice:
    """Simulates a GRBL controller behind a MockTransport.

    Responds to `?` polls with a status report, acknowledges every
    command line with `ok`, and supports canned responses for `$I`,
    `$$`, `$#`, `$G` and `G38.2` probe commands.
    """

    def __init__(self, mock, status=STATUS_IDLE, welcome=WELCOME):
        self.mock = mock
        self.status = status
        self.welcome = welcome
        self.settings = list(DEFAULT_SETTINGS)
        self.wcs_lines = list(DEFAULT_WCS_LINES)
        self.parser_state = "[G54 G17 G21 G90 G94 M5 M9 T0 F0 S0]"
        self.probe_response = "[PRB:10.000,20.000,5.000:1]"
        self.build_info = b"[VER:1.1h:]\r\n[OPT:V,15,127]\r\nok\r\n"
        self.auto_ok = True
        self.ack_delay_ticks = 0
        self.silent = False
        self.command_failures = {}
        self._seen = 0
        self._welcome_sent = welcome is None

    async def run(self):
        while True:
            sent = self.mock.sent()
            if len(sent) < self._seen:
                # The test cleared the sent log; start over.
                self._seen = 0
            for chunk in sent[self._seen :]:
                self._handle_chunk(chunk)
            self._seen = len(sent)
            await asyncio.sleep(0.005)

    def _respond(self, data):
        if self.ack_delay_ticks:
            asyncio.get_running_loop().call_later(
                self.ack_delay_ticks * 0.01,
                lambda: self.mock.push(data),
            )
        else:
            self.mock.push(data)

    def _handle_chunk(self, chunk):
        if self.silent:
            return
        if not self._welcome_sent:
            self._welcome_sent = True
            if self.welcome:
                self.mock.push(self.welcome)
        if chunk == b"?":
            self._respond(self.status)
            return
        if chunk in (b"\x18", b"!", b"~"):
            return
        text = chunk.decode("ascii", errors="replace")
        for line in text.splitlines():
            self._handle_line(line.strip())

    def _handle_line(self, line):
        if not line:
            return
        if line == "$I":
            self.mock.push(self.build_info)
            return
        if line == "$$":
            data = "".join(f"{s}\r\n" for s in self.settings)
            self.mock.push(data.encode() + b"ok\r\n")
            return
        if line == "$#":
            data = "".join(f"{s}\r\n" for s in self.wcs_lines)
            self.mock.push(data.encode() + b"ok\r\n")
            return
        if line == "$G":
            self.mock.push((self.parser_state + "\r\n").encode() + b"ok\r\n")
            return
        if line.startswith("G38.2"):
            self.mock.push((self.probe_response + "\r\n").encode() + b"ok\r\n")
            return
        failure = self.command_failures.get(line)
        if failure is not None:
            self.mock.push((failure + "\r\n").encode())
            return
        if self.auto_ok:
            self._respond(b"ok\r\n")


def make_session(mock, events, config=None, dialect=None):
    config = {**FAST_TIMINGS, **(config or {})}
    return GrblSession.with_transport(
        mock,
        config=config,
        dialect={**DEFAULT_DIALECT, **(dialect or {})},
        event_callback=events.callback,
    )


@pytest.fixture
def events():
    return EventRecorder()


@pytest.fixture
def mock():
    return MockTransport()


@pytest.fixture
def device(mock):
    return FakeDevice(mock)


@pytest.fixture
async def connected(mock, device, events):
    """A connected session with a responsive fake device.

    Yields ``(session, mock, device, events)`` with the handshake
    already done and the sent-bytes log cleared.
    """
    session = make_session(mock, events)
    device_task = asyncio.ensure_future(device.run())
    await session.connect()
    await events.wait_for(
        "connection_status_changed", lambda p: p[0] == "CONNECTED"
    )
    mock.clear_sent()
    events.events.clear()
    try:
        yield session, mock, device, events
    finally:
        await session.disconnect()
        device_task.cancel()
        try:
            await device_task
        except asyncio.CancelledError:
            pass


def sent_text(session_mock):
    return b"".join(session_mock.sent())
