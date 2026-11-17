import asyncio

import pytest

from raydriver.core.discovery import mdns_channel
from raydriver.core.discovery.fingerprints import TcpFingerprint
from raydriver.core.discovery.mdns_channel import find_network_devices
from raydriver.core.discovery.mdns_scan import MDNSService
from raydriver.core.discovery.spec import DiscoverySpec, MdnsRecognizer


async def _start_server(handler):
    server = await asyncio.start_server(handler, "127.0.0.1", 0)
    port = server.sockets[0].getsockname()[1]
    return server, port


async def _stop_server(server):
    server.close()
    await server.wait_closed()


@pytest.mark.asyncio
async def test_tcp_fingerprint_probes_with_command():
    """A command/reply probe claims a listener that answers with the
    matching line."""
    seen = []

    async def handler(reader, writer):
        try:
            seen.append(await reader.readline())
            writer.write(b"[VER:1.1h.20240101:]\r\n[OPT:V,15,128]\r\n")
            await writer.drain()
            await reader.read()
        finally:
            writer.close()

    server, port = await _start_server(handler)
    fingerprint = TcpFingerprint(
        probe_command="$I\n",
        match_prefix="[VER:",
        banner_wait=0.1,
        timeout=1.0,
    )
    try:
        matched = await fingerprint("127.0.0.1", port)
    finally:
        await _stop_server(server)
    assert matched
    assert seen == [b"$I\n"]


@pytest.mark.asyncio
async def test_tcp_fingerprint_accepts_unsolicited_banner():
    """A listener that announces itself on connect is claimed without
    sending the probe command."""

    async def handler(reader, writer):
        try:
            writer.write(b"[VER:1.1h.20240101:]\r\n")
            await writer.drain()
            await reader.read()
        finally:
            writer.close()

    server, port = await _start_server(handler)
    fingerprint = TcpFingerprint(
        probe_command="$I\n",
        match_prefix="[VER:",
        banner_wait=0.5,
        timeout=1.0,
    )
    try:
        matched = await fingerprint("127.0.0.1", port)
    finally:
        await _stop_server(server)
    assert matched


@pytest.mark.asyncio
async def test_tcp_fingerprint_rejects_other_listener():
    """An unrelated service answering the probe is not claimed."""

    async def handler(reader, writer):
        try:
            await reader.readline()
            writer.write(b"Welcome to the telnet console\r\n")
            await writer.drain()
            await reader.read()
        finally:
            writer.close()

    server, port = await _start_server(handler)
    fingerprint = TcpFingerprint(
        probe_command="$I\n",
        match_prefix="[VER:",
        banner_wait=0.1,
        timeout=0.5,
    )
    try:
        matched = await fingerprint("127.0.0.1", port)
    finally:
        await _stop_server(server)
    assert matched is None


@pytest.mark.asyncio
async def test_tcp_fingerprint_claim_via_mdns_channel(monkeypatch):
    """A candidate offered by the network channel is confirmed by the
    same probe and becomes a discovered device."""

    async def handler(reader, writer):
        try:
            await reader.readline()
            writer.write(b"[VER:1.1h.20240101:]\r\n")
            await writer.drain()
            await reader.read()
        finally:
            writer.close()

    server, port = await _start_server(handler)

    async def fake_scan(service_types):
        return [
            MDNSService(
                service_type="_telnet._tcp",
                name="grblHAL",
                host="127.0.0.1",
                port=port,
                server="grblhal.local",
                txt={},
            )
        ]

    class TelnetDriver:
        DISCOVERY = DiscoverySpec(
            mdns=MdnsRecognizer(
                fingerprint_services=("_telnet._tcp",),
                fingerprint=TcpFingerprint(
                    probe_command="$I\n",
                    match_prefix="[VER:",
                    banner_wait=0.1,
                    timeout=1.0,
                ),
            )
        )
        label = "grblHAL (Telnet)"

    monkeypatch.setattr(mdns_channel, "scan_mdns_services", fake_scan)
    try:
        devices = await find_network_devices([TelnetDriver])
    finally:
        await _stop_server(server)
    assert len(devices) == 1
    assert devices[0].params == {"host": "127.0.0.1", "port": port}
