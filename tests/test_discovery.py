"""Tests for the transport-level discovery scanners.

Exercises the Rust scanners through their PyO3 bindings: subnet
host enumeration and TCP scanning, serial capture against a pty,
and an mDNS browse against a locally announced service.
"""

import asyncio
import ipaddress
import os

import pytest

from raydriver.core import discovery


class TestSubnetHosts:
    def test_class_c_excludes_network_and_broadcast(self):
        hosts = discovery.subnet_hosts("192.168.1.5", 24)
        assert len(hosts) == 254
        assert hosts[0] == "192.168.1.1"
        assert hosts[-1] == "192.168.1.254"
        assert "192.168.1.0" not in hosts
        assert "192.168.1.255" not in hosts

    def test_wide_network_narrowed_to_class_c(self):
        hosts = discovery.subnet_hosts("10.1.2.3", 16)
        assert len(hosts) == 254
        assert hosts[0] == "10.1.2.1"
        assert hosts[-1] == "10.1.2.254"

    def test_host_route_yields_single_address(self):
        assert discovery.subnet_hosts("10.0.0.7", 32) == ["10.0.0.7"]

    def test_point_to_point_yields_both_addresses(self):
        assert discovery.subnet_hosts("10.0.0.0", 31) == [
            "10.0.0.0",
            "10.0.0.1",
        ]

    def test_invalid_address_raises(self):
        with pytest.raises(ValueError):
            discovery.subnet_hosts("not-an-ip", 24)


class TestLocalSubnets:
    def test_returns_parseable_non_loopback_interfaces(self):
        subnets = discovery.local_subnets()
        assert isinstance(subnets, list)
        for address, prefixlen in subnets:
            addr = ipaddress.IPv4Address(address)
            assert not addr.is_loopback
            assert 0 <= prefixlen <= 32


class TestScanSubnet:
    async def test_finds_listening_host_only(self):
        server = await asyncio.start_server(
            lambda r, w: w.close(), "127.0.0.1", 0
        )
        port = server.sockets[0].getsockname()[1]
        try:
            # 127.0.0.2 is loopback too, but nothing listens there.
            reachable = await discovery.scan_subnet(
                ["127.0.0.1", "127.0.0.2"], port, 0.5, 8
            )
            assert reachable == ["127.0.0.1"]
        finally:
            server.close()
            await server.wait_closed()

    async def test_empty_hosts(self):
        assert await discovery.scan_subnet([], 23, 0.1, 4) == []


class TestScanSerialPorts:
    async def test_absent_port_yields_nothing(self):
        observations = await discovery.scan_serial_ports(
            ["/dev/raydriver-does-not-exist"], [115200], 5.0
        )
        assert observations == []

    @pytest.mark.skipif(
        not hasattr(os, "openpty"), reason="requires a POSIX pty"
    )
    async def test_captures_device_banner(self):
        master, slave = os.openpty()
        try:
            os.write(master, b"Grbl 1.1h ['$' for help]\n")
            observations = await discovery.scan_serial_ports(
                [os.ttyname(slave)], [115200], 10.0
            )
            assert len(observations) == 1
            observation = observations[0]
            assert observation["baud_rate"] == 115200
            assert b"Grbl 1.1h" in observation["data"]
        finally:
            os.close(master)
            os.close(slave)


class TestScanMdnsServices:
    async def test_resolves_announced_service(self):
        zeroconf = pytest.importorskip("zeroconf")
        from zeroconf.asyncio import AsyncZeroconf

        service_type = "_rdisctest._tcp.local."
        info = zeroconf.ServiceInfo(
            service_type,
            f"disc.{service_type}",
            port=12345,
            addresses=[b"\x7f\x00\x00\x01"],
            properties={b"version": b"1.2.3"},
            server="disc.local.",
        )
        aiozc = AsyncZeroconf()
        await aiozc.async_register_service(info)
        try:
            services = await discovery.scan_mdns_services(
                ["_rdisctest._tcp"], 2.0
            )
            match = [s for s in services if s["name"] == "disc"]
            assert len(match) == 1
            service = match[0]
            assert service["service_type"] == "_rdisctest._tcp"
            assert service["host"] == "127.0.0.1"
            assert service["port"] == 12345
            assert service["server"] == "disc.local"
            assert service["txt"]["version"] == "1.2.3"
        finally:
            await aiozc.async_unregister_service(info)
            await aiozc.async_close()

    async def test_empty_types_yield_nothing(self):
        assert await discovery.scan_mdns_services([], 0.1) == []
