"""Tests for the daemon CLI, config handling and mDNS announce."""

import json
import socket
import subprocess
import sys

import pytest

from raydriver.cli.daemon import (
    config_from_dict,
    load_config,
    write_setup_config,
)
from raydriver.remote import mdns
from raydriver.remote.daemon import DaemonConfig


def test_setup_writes_token_config(tmp_path):
    path = tmp_path / "sub" / "daemon.json"
    token = write_setup_config(path)
    assert path.exists()
    assert (path.stat().st_mode & 0o777) == 0o600
    data = json.loads(path.read_text())
    assert data["token"] == token
    assert data["host"] == "127.0.0.1"
    with pytest.raises(FileExistsError):
        write_setup_config(path)


def test_config_from_dict_defaults():
    config = config_from_dict({})
    defaults = DaemonConfig()
    assert config.host == defaults.host
    assert config.port == defaults.port
    assert config.keepalive_interval == defaults.keepalive_interval
    assert config.keepalive_misses == defaults.keepalive_misses
    assert config.mdns is False


def test_config_from_dict_overrides():
    config = config_from_dict(
        {
            "host": "0.0.0.0",
            "port": 9999,
            "token": "t",
            "tls_cert": "c.pem",
            "tls_key": "k.pem",
            "keepalive_interval": 1.5,
            "keepalive_misses": 5,
            "lease_grace": 2.5,
            "ring_size": 50,
            "cameras": [
                {
                    "name": "cam",
                    "uri": "http://x/stream",
                    "type": "http_stream",
                }
            ],
            "sessions": [
                {"protocol": "grbl", "config": {"port": "/dev/ttyUSB0"}}
            ],
            "mdns": True,
        }
    )
    assert config.host == "0.0.0.0"
    assert config.port == 9999
    assert config.token == "t"
    assert config.tls_cert == "c.pem"
    assert config.keepalive_interval == 1.5
    assert config.keepalive_misses == 5
    assert config.lease_grace == 2.5
    assert config.ring_size == 50
    assert config.cameras[0]["name"] == "cam"
    assert config.sessions[0]["protocol"] == "grbl"
    assert config.mdns is True


def test_load_config(tmp_path):
    path = tmp_path / "daemon.json"
    path.write_text(json.dumps({"port": 1234}))
    assert load_config(path).port == 1234


def test_cli_setup_and_serve(tmp_path):
    """End-to-end: python -m raydriver daemon --setup, then serve."""
    config = tmp_path / "daemon.json"
    setup = subprocess.run(
        [
            sys.executable,
            "-m",
            "raydriver",
            "daemon",
            "--setup",
            "--config",
            str(config),
        ],
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert setup.returncode == 0, setup.stderr
    assert "openssl" in setup.stdout
    assert config.exists()

    data = json.loads(config.read_text())
    data["port"] = 0
    config.write_text(json.dumps(data))
    proc = subprocess.Popen(
        [
            sys.executable,
            "-m",
            "raydriver",
            "daemon",
            "--config",
            str(config),
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    try:
        stdout = proc.stdout
        assert stdout is not None
        # readline is portable (select on pipes is not, on Windows);
        # the daemon flushes the listening line, and a dead process
        # yields EOF. pytest-timeout backstops a silent hang.
        line = stdout.readline()
        assert "listening" in line, "daemon did not announce its listener"
        port = int(line.strip().rsplit(":", 1)[1])
        with socket.create_connection(("127.0.0.1", port), timeout=5) as sock:
            prefix = sock.recv(4)
            assert len(prefix) == 4
            size = int.from_bytes(prefix, "big")
            payload = b""
            while len(payload) < size:
                payload += sock.recv(size - len(payload))
            welcome = json.loads(payload)
        assert welcome["type"] == "welcome"
        assert welcome["payload"]["auth_required"] is True
    finally:
        proc.terminate()
        proc.wait(timeout=10)


async def test_mdns_without_extra(monkeypatch):
    monkeypatch.setattr(mdns, "Zeroconf", None)
    assert await mdns.announce(DaemonConfig(), 8765) is None


async def test_mdns_announce_registers_service():
    pytest.importorskip("zeroconf")
    config = DaemonConfig(
        token="t",
        cameras=[{"name": "c", "uri": "u", "type": "http_stream"}],
    )
    announcer = await mdns.announce(config, 8765)
    assert announcer is not None
    try:
        assert announcer.info.port == 8765
        assert announcer.info.properties[b"requires_auth"] == b"true"
        assert announcer.info.properties[b"tls"] == b"false"
        assert announcer.info.properties[b"cameras"] == b"1"
    finally:
        await announcer.close()
