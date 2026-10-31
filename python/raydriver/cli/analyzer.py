"""Analyzer CLI: capture, decode, replay and proxy device traffic.

    raydriver analyzer record --port /dev/ttyUSB0 --out cap.jsonl
    raydriver analyzer record --emulator            # hardware-free
    raydriver analyzer tui --port /dev/ttyUSB0      # live monitor
    raydriver analyzer decode cap.jsonl
    raydriver analyzer replay cap.jsonl --to 192.168.1.50:23
    raydriver analyzer proxy --upstream 192.168.1.50:23 --out mitm.jsonl

Recording attaches to a GRBL session and logs every line both
ways; the proxy instead sits between a network application and a
TCP device (the man-in-the-middle workflow for characterizing
controllers and other applications).  Network endpoints follow the
analyzer security policy: loopback by default, token and TLS
required for exposed listeners.
"""

import argparse
import asyncio
import sys
import time

from raydriver.analyzer.capture import Capture, Frame, Recorder
from raydriver.analyzer.decode import (
    decoder_names,
    format_frame,
    get_decoder,
    render_listing,
)
from raydriver.analyzer.listener import connect_authenticated
from raydriver.analyzer.proxy import (
    FRAMING_LINES,
    FRAMING_RAW,
    run_proxy,
)
from raydriver.analyzer.replay import replay_capture
from raydriver.core.transport import MockTransport
from raydriver.grbl import GrblSession


def _parse_endpoint(value: str) -> tuple[str, int]:
    host, _, port = value.rpartition(":")
    if not host or not port.isdigit():
        raise argparse.ArgumentTypeError(f"expected HOST:PORT, got {value!r}")
    return host, int(port)


def _open_recording_session(args, recorder):
    """A GrblSession with the recorder attached, against the
    emulator or a serial port.  Returns (session, device_task,
    source label)."""
    config = {"poll_status_while_running": not args.no_poll}
    if args.emulator:
        from raydriver.grbl.emulator import GrblEmulator

        mock = MockTransport()
        emulator = GrblEmulator(mock, speed_factor=args.speed_factor)
        session = GrblSession.with_transport(
            mock, config=config, dialect={}, event_callback=recorder
        )
        return (
            session,
            asyncio.ensure_future(emulator.run()),
            "emulated GRBL device",
        )
    session = GrblSession(
        config={
            "port": args.port,
            "baudrate": args.baudrate,
            **config,
        },
        dialect={},
        event_callback=recorder,
    )
    return session, None, f"{args.port} @ {args.baudrate}"


async def _print_new_frames(recorder, decoder, printed: int) -> int:
    for index in range(printed, len(recorder.capture.frames)):
        frame = recorder.capture.frames[index]
        print(format_frame(index, frame, decoder), flush=True)
    return len(recorder.capture.frames)


def _finish(recorder, args) -> None:
    if args.out:
        recorder.capture.save(args.out)
        print(
            f"saved {len(recorder.capture.frames)} frames to {args.out}",
            file=sys.stderr,
        )


async def cmd_record(args) -> int:
    recorder = Recorder()
    decoder = get_decoder(args.protocol)
    session, device_task, source = _open_recording_session(args, recorder)
    recorder.capture.protocol = args.protocol
    recorder.capture.source = source
    printed = 0
    print(f"Recording {source} — Ctrl-C to stop", file=sys.stderr)
    try:
        await session.connect()
        deadline = (
            time.monotonic() + args.seconds if args.seconds > 0 else None
        )
        while deadline is None or time.monotonic() < deadline:
            printed = await _print_new_frames(recorder, decoder, printed)
            await asyncio.sleep(0.05)
    except asyncio.CancelledError:
        pass
    finally:
        try:
            await asyncio.shield(session.disconnect())
        except Exception:  # noqa: BLE001 - teardown best effort
            pass
        if device_task is not None:
            device_task.cancel()
        _finish(recorder, args)
    return 0


async def cmd_tui(args) -> int:
    try:
        from raydriver.analyzer.tui import run_tui
    except ImportError as err:
        print(f"error: TUI unavailable: {err}", file=sys.stderr)
        return 2
    recorder = Recorder()
    decoder = get_decoder(args.protocol)
    session, device_task, source = _open_recording_session(args, recorder)
    recorder.capture.protocol = args.protocol
    recorder.capture.source = source

    async def work():
        try:
            await session.connect()
            while True:
                await asyncio.sleep(1.0)
        finally:
            try:
                await session.disconnect()
            except Exception:  # noqa: BLE001 - teardown best effort
                pass
            if device_task is not None:
                device_task.cancel()

    run_tui(
        recorder,
        decoder,
        source=source,
        save_path=args.out,
        work=work(),
    )
    return 0


async def cmd_decode(args) -> int:
    capture = Capture.load(args.file)
    decoder = get_decoder(args.protocol or capture.protocol)
    sys.stdout.write(
        render_listing(capture, decoder, with_times=not args.no_times)
    )
    return 0


async def cmd_replay(args) -> int:
    capture = Capture.load(args.file)
    decoder = get_decoder(args.protocol or capture.protocol)
    host, port = args.to
    print(
        f"Replaying {len(capture.frames)} frames to "
        f"{host}:{port} (speed {args.speed:g})",
        file=sys.stderr,
    )
    reader, writer = await connect_authenticated(
        host,
        port,
        token=args.token,
        tls_ca=args.tls_ca,
        tls_hostname=args.tls_hostname,
    )
    started = time.monotonic()
    responses: list[str] = []

    async def collect_responses():
        while True:
            line = await reader.readline()
            if not line:
                return
            responses.append(line.decode(errors="replace").rstrip("\r\n"))

    collector = asyncio.ensure_future(collect_responses())

    async def send(data):
        writer.write(data)
        await writer.drain()

    try:
        written = await replay_capture(capture, send, speed=args.speed)
        print(
            f"sent {written} frames; collecting responses ...", file=sys.stderr
        )
        await asyncio.sleep(args.wait)
    finally:
        collector.cancel()
        writer.close()
    for index, line in enumerate(responses):
        frame = Frame("rx", time.monotonic() - started, text=line)
        print(format_frame(index, frame, decoder), flush=True)
    return 0


async def cmd_proxy(args) -> int:
    upstream_host, upstream_port = args.upstream
    recorder = Recorder()
    recorder.capture.protocol = args.protocol
    recorder.capture.source = f"proxy -> {args.upstream[0]}:{args.upstream[1]}"
    decoder = get_decoder(args.protocol)
    proxy = await run_proxy(
        upstream_host,
        upstream_port,
        listen_host=args.listen_host,
        listen_port=args.listen_port,
        token=args.token,
        tls_cert=args.tls_cert,
        tls_key=args.tls_key,
        capture=recorder.capture,
        framing=args.framing,
    )
    print(
        f"listening on {args.listen_host}:{proxy.port}, forwarding to "
        f"{upstream_host}:{upstream_port} — Ctrl-C to stop",
        file=sys.stderr,
    )
    printed = 0
    try:
        while True:
            printed = await _print_new_frames(recorder, decoder, printed)
            await asyncio.sleep(0.05)
    except asyncio.CancelledError:
        pass
    finally:
        await proxy.stop()
        _finish(recorder, args)
    return 0


def add_analyzer_parser(subparsers) -> None:
    parser = subparsers.add_parser(
        "analyzer",
        help="capture, decode, replay and proxy device traffic",
    )
    analyzer_subparsers = parser.add_subparsers(
        dest="analyzer_command", required=True
    )

    def source_arguments(record_parser):
        record_parser.add_argument(
            "--port", default="/dev/ttyUSB0", help="serial port"
        )
        record_parser.add_argument(
            "--baudrate", type=int, default=115200, help="baud rate"
        )
        record_parser.add_argument(
            "--emulator",
            action="store_true",
            help="record the built-in GRBL firmware emulator "
            "instead of a serial port",
        )
        record_parser.add_argument(
            "--speed-factor",
            type=float,
            default=2000.0,
            help="emulator motion speed multiplier (emulator only)",
        )
        record_parser.add_argument(
            "--no-poll",
            action="store_true",
            help="disable status polling while jobs run",
        )
        record_parser.add_argument(
            "--protocol",
            default="grbl",
            choices=decoder_names(),
            help="decoder used for live annotations and captures",
        )
        record_parser.add_argument(
            "--out", default=None, help="save the capture as JSONL"
        )

    record_parser = analyzer_subparsers.add_parser(
        "record", help="record a session to a capture file"
    )
    source_arguments(record_parser)
    record_parser.add_argument(
        "--seconds",
        type=float,
        default=10.0,
        help="recording duration (0: until Ctrl-C)",
    )
    record_parser.set_defaults(func=cmd_record)

    tui_parser = analyzer_subparsers.add_parser(
        "tui", help="interactive live monitor (needs a terminal)"
    )
    source_arguments(tui_parser)
    tui_parser.set_defaults(func=cmd_tui)

    decode_parser = analyzer_subparsers.add_parser(
        "decode", help="print a capture as a decoded listing"
    )
    decode_parser.add_argument("file", help="capture file (JSONL)")
    decode_parser.add_argument(
        "--protocol",
        default=None,
        choices=decoder_names(),
        help="decoder (default: the capture's protocol)",
    )
    decode_parser.add_argument(
        "--no-times",
        action="store_true",
        help="omit the timestamp column",
    )
    decode_parser.set_defaults(func=cmd_decode)

    replay_parser = analyzer_subparsers.add_parser(
        "replay", help="replay captured frames to a device"
    )
    replay_parser.add_argument("file", help="capture file (JSONL)")
    replay_parser.add_argument(
        "--to",
        type=_parse_endpoint,
        required=True,
        help="device endpoint, HOST:PORT",
    )
    replay_parser.add_argument(
        "--speed",
        type=float,
        default=1.0,
        help="replay speed multiplier (0: as fast as possible)",
    )
    replay_parser.add_argument(
        "--wait",
        type=float,
        default=2.0,
        help="seconds to collect responses after replaying",
    )
    replay_parser.add_argument(
        "--token", default=None, help="token for the target endpoint"
    )
    replay_parser.add_argument(
        "--tls-ca", default=None, help="CA file verifying the target"
    )
    replay_parser.add_argument(
        "--tls-hostname",
        default=None,
        help="expected TLS name of the target",
    )
    replay_parser.add_argument(
        "--protocol",
        default=None,
        choices=decoder_names(),
        help="decoder (default: the capture's protocol)",
    )
    replay_parser.set_defaults(func=cmd_replay)

    proxy_parser = analyzer_subparsers.add_parser(
        "proxy",
        help="run a man-in-the-middle proxy in front of a device",
    )
    proxy_parser.add_argument(
        "--upstream",
        type=_parse_endpoint,
        required=True,
        help="device endpoint, HOST:PORT",
    )
    proxy_parser.add_argument(
        "--listen-host",
        default="127.0.0.1",
        help="listen address (non-loopback requires --token and TLS)",
    )
    proxy_parser.add_argument(
        "--listen-port", type=int, default=0, help="listen port"
    )
    proxy_parser.add_argument(
        "--token",
        default=None,
        help="require this token from connecting clients",
    )
    proxy_parser.add_argument(
        "--tls-cert", default=None, help="TLS certificate file"
    )
    proxy_parser.add_argument("--tls-key", default=None, help="TLS key file")
    proxy_parser.add_argument(
        "--framing",
        default=FRAMING_LINES,
        choices=[FRAMING_LINES, FRAMING_RAW],
        help="record lines (text protocols) or raw chunks",
    )
    proxy_parser.add_argument(
        "--protocol",
        default="raw",
        choices=decoder_names(),
        help="decoder used for live annotations and captures",
    )
    proxy_parser.add_argument(
        "--out", default=None, help="save the capture as JSONL"
    )
    proxy_parser.set_defaults(func=cmd_proxy)
