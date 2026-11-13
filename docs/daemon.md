# raydriver daemon

The daemon hosts raydriver sessions next to the hardware (e.g. on
a Raspberry Pi in the workshop) while Rayforge — or any raydriver
consumer — runs elsewhere on the LAN. Jobs run daemon-side and
survive client disconnects; reattaching clients resync progress
exactly. The daemon hosts *sessions*, not machines: machine
configs and dialects stay client-side.

## Install and first run

```sh
pip install raydriver            # add [daemon] for mDNS announce
raydriver daemon --setup         # writes ~/.config/raydriver/daemon.json
raydriver daemon                 # serves the config
```

`--setup` generates a token (config written mode 0600) and prints
the certificate one-liner. `python -m raydriver daemon` works too.

## Config reference (`daemon.json`)

| Key                  | Default            | Meaning                              |
| -------------------- | ------------------ | ------------------------------------ |
| `host`               | `127.0.0.1`        | bind address                         |
| `port`               | `8765`             | bind port (`0` = ephemeral)          |
| `token`              | none               | shared secret, constant-time checked |
| `tls_cert`/`tls_key` | none               | PEM certificate and key              |
| `keepalive_interval` | `2.0`              | seconds between pings                |
| `keepalive_misses`   | `3`                | missed pings before disconnect       |
| `lease_grace`        | `5.0`              | seconds a disconnected holder keeps the control lease |
| `ring_size`          | `1000`             | replayable events per session        |
| `cameras`            | `[]`               | advertised stream endpoints (below)  |
| `sessions`           | `[]`               | sessions to auto-open at boot        |
| `mdns`               | `false`            | announce `_raydriver._tcp`           |

Auto-open example:

```json
{
  "host": "0.0.0.0",
  "token": "…",
  "tls_cert": "/etc/raydriver/daemon-cert.pem",
  "tls_key": "/etc/raydriver/daemon-key.pem",
  "sessions": [
    {"protocol": "grbl", "config": {"port": "/dev/ttyUSB0", "baudrate": 115200}}
  ]
}
```

## Security policy

The analyzer's bind policy applies unchanged: loopback may run
without auth; a non-loopback or wildcard bind refuses to open
without **both** a token and TLS; explicitly configured auth is
never dropped. Clients refuse to send a token over a non-TLS
connection. Generate a certificate with:

```sh
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout daemon-key.pem -out daemon-cert.pem -days 365 \
  -subj /CN=raydriver \
  -addext subjectAltName=DNS:localhost,IP:127.0.0.1
```

Add the LAN hostname/IP to `subjectAltName` for remote clients,
and pin the certificate on the client (`tls_ca`). Keep the daemon
port firewalled to the LAN; there is no WAN/NAT traversal.

## systemd

```ini
[Unit]
Description=raydriver device daemon
After=network-online.target

[Service]
ExecStart=/usr/bin/raydriver daemon --config /etc/raydriver/daemon.json
Restart=on-failure
User=raydriver
# Serial access:
SupplementaryGroups=dialout

[Install]
WantedBy=multi-user.target
```

## Cameras on the device host

Video never crosses the control protocol. The daemon only
*advertises* camera endpoints; Rayforge's existing network-camera
sources (`http_stream`, `http_snapshot`, `rtsp`) consume them.

- IP cameras: add the camera's own stream URL in Rayforge.
- USB cameras on the host: run a standard streamer, e.g.
  `ustreamer --device /dev/video0 --host 0.0.0.0 --port 8080`,
  then advertise it:

```json
{
  "cameras": [
    {"name": "enclosure", "uri": "http://pi.local:8080/stream", "type": "http_stream"}
  ]
}
```

The endpoints surface in `device/list` and the mDNS TXT records;
the Rayforge wizard and camera dialog prefill the URI. Keep video
endpoints LAN-only or firewalled — they are separate from the TLS
control channel.

## Clients

Rayforge's migrated drivers gain a remote endpoint group
(`remote_host`, `remote_port`, `token`, TLS pin), or a single URL:
`raydriver://host:8765#token`. Any raydriver consumer can use
`raydriver.remote.client.create_session` the same way.