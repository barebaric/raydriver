"""Read-only inspection of a running daemon (CLI debug commands).

Speaks the control protocol directly: welcome, optional auth, then
a batch of requests. Used by ``raydriver info/list/connection`` and
``raydriver daemon status``; never mutates daemon state.
"""

from __future__ import annotations

import asyncio
from typing import Any

from raydriver.analyzer.listener import make_client_ssl_context
from raydriver.remote import protocol


async def _read_reply(reader: asyncio.StreamReader) -> dict[str, Any]:
    """Read one response frame, skipping keepalive pings."""
    while True:
        msg = await protocol.read_frame(reader)
        if msg.get("type") == protocol.PING:
            continue
        return msg


async def daemon_rpc(
    host: str,
    port: int,
    token: str | None,
    tls_ca: str | None,
    requests: list[tuple[str, dict[str, Any]]],
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    """Connect, authenticate and run *requests*.

    Returns ``(welcome_payload, replies)``. Raises ConnectionError
    when the endpoint is not a reachable, authenticating daemon.
    A token is refused without TLS except on loopback, matching the
    analyzer's policy for local diagnostic endpoints.
    """
    loopback = host in ("127.0.0.1", "::1", "localhost")
    if token and not tls_ca and not loopback:
        raise ConnectionError(
            "refusing to send a token without TLS (use --tls-ca)"
        )
    ssl_context = make_client_ssl_context(tls_ca) if tls_ca else None
    reader, writer = await asyncio.open_connection(
        host,
        port,
        ssl=ssl_context,
        server_hostname=host if ssl_context else None,
    )
    try:
        welcome = await asyncio.wait_for(
            protocol.read_frame(reader), timeout=5.0
        )
        if welcome.get("type") != protocol.WELCOME:
            raise ConnectionError("not a raydriver daemon")
        payload = welcome["payload"]
        if payload.get("auth_required"):
            await protocol.write_frame(
                writer,
                protocol.request(0, protocol.AUTH, {"token": token or ""}),
            )
            reply = await asyncio.wait_for(_read_reply(reader), timeout=5.0)
            if not reply.get("ok"):
                raise ConnectionError("daemon authentication failed")
        replies = []
        for index, (type_, req_payload) in enumerate(requests, start=1):
            await protocol.write_frame(
                writer, protocol.request(index, type_, req_payload)
            )
            replies.append(
                await asyncio.wait_for(_read_reply(reader), timeout=5.0)
            )
        return payload, replies
    finally:
        writer.close()


async def welcome_probe(host: str, port: int) -> dict[str, Any]:
    """Read just the welcome frame — no credentials needed."""
    reader, writer = await asyncio.open_connection(host, port)
    try:
        welcome = await asyncio.wait_for(
            protocol.read_frame(reader), timeout=5.0
        )
        if welcome.get("type") != protocol.WELCOME:
            raise ConnectionError("not a raydriver daemon")
        return welcome["payload"]
    finally:
        writer.close()


async def snapshot(
    host: str,
    port: int,
    token: str | None = None,
    tls_ca: str | None = None,
) -> dict[str, Any]:
    """Welcome plus sessions, clients and devices in one call."""
    welcome, replies = await daemon_rpc(
        host,
        port,
        token,
        tls_ca,
        [
            (protocol.SESSION_LIST, {}),
            (protocol.CLIENT_LIST, {}),
            (protocol.DEVICE_LIST, {}),
        ],
    )

    def _payload(reply: dict[str, Any], key: str) -> Any:
        if not reply.get("ok"):
            return []
        return reply["payload"].get(key, [])

    return {
        "welcome": welcome,
        "sessions": _payload(replies[0], "sessions"),
        "clients": _payload(replies[1], "clients"),
        "devices": _payload(replies[2], "ports"),
        "cameras": _payload(replies[2], "cameras"),
    }
