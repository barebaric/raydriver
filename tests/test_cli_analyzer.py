"""Tests for the raydriver analyzer CLI."""

import asyncio
import json
from pathlib import Path

from raydriver.cli.analyzer import (
    _parse_endpoint,
    cmd_decode,
    cmd_replay,
)
from raydriver.cli.main import build_parser

GOLDEN = Path(__file__).parent / "golden"


def run_cli(argv):
    args = build_parser().parse_args(argv)
    return asyncio.run(args.func(args))


def test_parse_endpoint():
    assert _parse_endpoint("192.168.1.5:23") == ("192.168.1.5", 23)
    assert _parse_endpoint("host.example:8080") == (
        "host.example",
        8080,
    )


def test_parser_wires_analyzer_subcommands():
    parser = build_parser()
    args = parser.parse_args(["analyzer", "decode", "cap.jsonl"])
    assert args.func is cmd_decode
    args = parser.parse_args(
        ["analyzer", "proxy", "--upstream", "h:1", "--out", "x.jsonl"]
    )
    assert args.upstream == ("h", 1)
    assert args.listen_host == "127.0.0.1"
    assert "analyzer" in parser.format_help()


def test_decode_prints_listing(capsys):
    code = run_cli(["analyzer", "decode", str(GOLDEN / "grbl-session.jsonl")])
    assert code == 0
    out = capsys.readouterr().out
    assert "build info request" in out
    assert "ack" in out


def test_record_emulator_writes_capture(tmp_path, capsys):
    out = tmp_path / "cap.jsonl"
    code = run_cli(
        [
            "analyzer",
            "record",
            "--emulator",
            "--seconds",
            "0.5",
            "--out",
            str(out),
        ]
    )
    assert code == 0
    lines = out.read_text().splitlines()
    meta = json.loads(lines[0])["meta"]
    assert meta["protocol"] == "grbl"
    frames = [json.loads(line) for line in lines[1:]]
    assert any(frame["d"] == "tx" for frame in frames)
    assert any(frame["d"] == "rx" for frame in frames)


async def test_replay_command_against_fake_device(tmp_path):
    async def grblish_device(reader, writer):
        try:
            while True:
                line = await reader.readline()
                if not line:
                    return
                if line.decode().strip() == "$I":
                    writer.write(b"[VER:1.1h.20190825:]\nok\n")
                else:
                    writer.write(b"ok\n")
                await writer.drain()
        finally:
            writer.close()

    device = await asyncio.start_server(grblish_device, "127.0.0.1", 0)
    port = device.sockets[0].getsockname()[1]
    capture_path = tmp_path / "cap.jsonl"
    from raydriver.analyzer.capture import Capture

    capture = Capture(protocol="grbl")
    capture.append("tx", text="$I")
    capture.append("tx", text="G1 X10")
    capture.save(capture_path)

    class Args:
        file = str(capture_path)
        to = ("127.0.0.1", port)
        speed = 0.0
        wait = 0.3
        token = None
        tls_ca = None
        tls_hostname = None
        protocol = None

    try:
        assert await cmd_replay(Args()) == 0
    finally:
        device.close()
        await device.wait_closed()
