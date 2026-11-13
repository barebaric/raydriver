"""Debug CLI: inspect devices, daemons and connections.

    raydriver scan [--network]   # local serial ports (+ daemons)
    raydriver info TARGET        # a serial port or a host:port daemon
    raydriver list               # configured devices + live state
    raydriver connection         # clients connected to the daemon

Every command accepts --json for machine-readable output. Daemon
commands read the endpoint and credentials from the daemon config
(--config to override, --token/--tls-ca to override credentials).
"""

from __future__ import annotations

import argparse
import asyncio
import json
import sys
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit

from raydriver.cli.daemon import load_config
from raydriver.core import list_serial_ports
from raydriver.paths import default_config_path
from raydriver.remote import inspect as rins
from raydriver.remote import mdns
from raydriver.remote.daemon import DaemonConfig

_WILDCARD_HOSTS = ("0.0.0.0", "::", "*", "")


def _config_path(args: argparse.Namespace) -> Path:
    return Path(args.config) if args.config else default_config_path()


def _load_config(args: argparse.Namespace) -> DaemonConfig:
    """The daemon config, or defaults when no config file exists yet
    (so the debug commands work before ``daemon --setup``)."""
    path = _config_path(args)
    if not path.exists():
        return DaemonConfig()
    return load_config(path)


def _endpoint(
    args: argparse.Namespace,
) -> tuple[str, int, str | None, str | None]:
    config = _load_config(args)
    host = config.host
    if host in _WILDCARD_HOSTS:
        host = "127.0.0.1"
    token = args.token if args.token is not None else config.token
    return host, config.port, token, args.tls_ca


def _emit(args: argparse.Namespace, data: Any, rows: list[str]) -> None:
    if args.json:
        print(json.dumps(data, indent=2))
    else:
        for line in rows:
            print(line)


def _port_line(port: dict[str, Any]) -> str:
    vid = port.get("vid")
    pid = port.get("pid")
    vidpid = f"{vid:04x}:{pid:04x}" if vid is not None else "----:----"
    product = port.get("product") or ""
    maker = port.get("manufacturer") or ""
    return f"  {port['port']}  {vidpid}  {product}  {maker}".rstrip()


async def cmd_scan(args: argparse.Namespace) -> int:
    ports = list_serial_ports()
    daemons: list[dict[str, Any]] = []
    if args.network:
        daemons = await mdns.browse_daemons()
    rows = ["Serial ports:"]
    if not ports:
        rows.append("  (none found)")
    rows.extend(_port_line(port) for port in ports)
    if args.network:
        rows.append("Daemons (_raydriver._tcp):")
        if not daemons:
            rows.append("  (none found)")
        for daemon in daemons:
            rows.append(
                f"  {daemon['name']}  {daemon['host']}:{daemon['port']}"
            )
    _emit(args, {"ports": ports, "daemons": daemons}, rows)
    return 0


def _parse_endpoint(target: str) -> tuple[str | None, int, str | None]:
    if target.startswith("raydriver://"):
        parts = urlsplit(target)
        return parts.hostname, parts.port or 8765, parts.fragment or None
    host, sep, port = target.rpartition(":")
    if sep and host and port.isdigit():
        return host, int(port), None
    return None, 0, None


async def cmd_info(args: argparse.Namespace) -> int:
    host, port, url_token = _parse_endpoint(args.target)
    if host is None:
        for entry in list_serial_ports():
            if entry["port"] == args.target:
                rows = [f"{key}: {value}" for key, value in entry.items()]
                _emit(args, entry, rows)
                return 0
        print(f"error: no serial port {args.target!r} found", file=sys.stderr)
        return 1
    token = args.token if args.token is not None else url_token
    if token is None:
        token = _load_config(args).token
    try:
        snap = await rins.snapshot(host, port, token, args.tls_ca)
    except (ConnectionError, OSError, asyncio.TimeoutError) as err:
        print(f"error: {err}", file=sys.stderr)
        return 1
    welcome = snap["welcome"]
    rows = [
        f"Daemon {host}:{port}",
        f"  raydriver {welcome.get('raydriver')} "
        f"(protocol {welcome.get('protocol')})",
        f"  sessions: {len(snap['sessions'])}  "
        f"clients: {len(snap['clients'])}",
    ]
    for session in snap["sessions"]:
        rows.append(
            f"  session {session['session_id']} [{session['protocol']}] "
            f"{session.get('connection_status')} "
            f"config={session.get('config')}"
        )
    rows.append("  devices:")
    rows.extend(f"    {entry['port']}" for entry in snap["devices"])
    for camera in snap["cameras"]:
        rows.append(f"  camera {camera.get('name')}: {camera.get('uri')}")
    _emit(args, snap, rows)
    return 0


async def cmd_list(args: argparse.Namespace) -> int:
    path = _config_path(args)
    config = _load_config(args)
    host, port, token, tls_ca = _endpoint(args)
    live: list[dict[str, Any]] = []
    state = "daemon not running"
    try:
        snap = await rins.snapshot(host, port, token, tls_ca)
        live = snap["sessions"]
        state = "running"
    except (ConnectionError, OSError, asyncio.TimeoutError):
        pass
    rows = [f"Configured devices ({path}):"]
    if not config.sessions:
        rows.append("  (none configured)")
    matched: set[str] = set()
    for entry in config.sessions:
        entry_config = entry.get("config") or {}
        device = entry_config.get("port") or entry_config.get("host") or "?"
        match = next(
            (
                session
                for session in live
                if session["protocol"] == entry.get("protocol")
                and session.get("config") == entry_config
            ),
            None,
        )
        if match is not None:
            matched.add(match["session_id"])
            job = match.get("job")
            job_state = "job running" if job and job.get("running") else "idle"
            rows.append(
                f"  {entry.get('protocol')} {device}: "
                f"{match.get('connection_status')} ({job_state})"
            )
        else:
            rows.append(
                f"  {entry.get('protocol')} {device}: not open ({state})"
            )
    for session in live:
        if session["session_id"] not in matched:
            rows.append(
                f"  {session['protocol']} (ad-hoc {session['session_id']}): "
                f"{session.get('connection_status')}"
            )
    _emit(
        args,
        {"configured": config.sessions, "live": live, "daemon": state},
        rows,
    )
    return 0


async def cmd_connection(args: argparse.Namespace) -> int:
    host, port, token, tls_ca = _endpoint(args)
    try:
        snap = await rins.snapshot(host, port, token, tls_ca)
    except (ConnectionError, OSError, asyncio.TimeoutError) as err:
        print(
            f"error: cannot reach daemon at {host}:{port}: {err}",
            file=sys.stderr,
        )
        return 1
    rows = [f"Rayforge connections to {host}:{port}:"]
    if not snap["clients"]:
        rows.append("  (no clients connected)")
    for client in snap["clients"]:
        roles = (
            ", ".join(
                f"{session}:{role}"
                for session, role in client["roles"].items()
            )
            or "no sessions"
        )
        rows.append(
            f"  {client['address']} (id {client['id']}, "
            f"auth={client['authenticated']}) {roles}"
        )
    for session in snap["sessions"]:
        holder = session.get("lease_holder") or "-"
        rows.append(
            f"  session {session['session_id']}: lease={holder} "
            f"watchers={session.get('watchers')}"
        )
    _emit(args, snap, rows)
    return 0


def _add_daemon_options(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--token", default=None, help="daemon token")
    parser.add_argument(
        "--tls-ca", default=None, help="PEM file pinning the daemon cert"
    )
    parser.add_argument(
        "--json", action="store_true", help="machine-readable output"
    )


def _add_config_option(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "--config",
        default=None,
        help=f"daemon config file (default: {default_config_path()})",
    )


def add_debug_parser(subparsers) -> None:
    scan = subparsers.add_parser(
        "scan", help="list local serial ports (and daemons with --network)"
    )
    scan.add_argument(
        "--network",
        action="store_true",
        help="also browse for raydriver daemons via mDNS",
    )
    scan.add_argument(
        "--json", action="store_true", help="machine-readable output"
    )
    scan.set_defaults(func=cmd_scan)

    info = subparsers.add_parser(
        "info", help="show a serial port or a daemon (host:port)"
    )
    info.add_argument("target", help="serial port path or host:port")
    _add_config_option(info)
    _add_daemon_options(info)
    info.set_defaults(func=cmd_info)

    listing = subparsers.add_parser(
        "list", help="list configured devices with live connection state"
    )
    _add_config_option(listing)
    _add_daemon_options(listing)
    listing.set_defaults(func=cmd_list)

    connection = subparsers.add_parser(
        "connection", help="show Rayforge clients connected to the daemon"
    )
    _add_config_option(connection)
    _add_daemon_options(connection)
    connection.set_defaults(func=cmd_connection)
