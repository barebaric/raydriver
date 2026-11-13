# raydriver daemon

The daemon hosts raydriver sessions next to the hardware (e.g. on
a Raspberry Pi in the workshop) while Rayforge — or any raydriver
consumer — runs elsewhere on the LAN. Jobs run daemon-side and
survive client disconnects; reattaching clients resync progress
exactly. The daemon hosts *sessions*, not machines: machine
configs and dialects stay client-side.

Everything here works on Linux, macOS and Windows.

## Install and first run

```sh
pip install raydriver            # add [daemon] for mDNS announce
raydriver daemon --setup         # write a config with a generated token
raydriver daemon install         # install + start the platform service
```

`--setup` generates a token (config written mode 0600) and prints
the certificate one-liner. The config lives in the platform's
per-user location:

| Platform | Config path                                        |
| -------- | -------------------------------------------------- |
| Linux    | `~/.config/raydriver/daemon.json` (or `$XDG_CONFIG_HOME`) |
| macOS    | `~/Library/Application Support/raydriver/daemon.json` |
| Windows  | `%APPDATA%\raydriver\daemon.json`                  |

`--config PATH` overrides it everywhere; `python -m raydriver
daemon` works too. To run in the foreground instead of as a
service: `raydriver daemon`.

## Service installation

`raydriver daemon install` uses the platform's native mechanism:

- **Linux**: a systemd *user* unit
  (`~/.config/systemd/user/raydriver-daemon.service`), enabled and
  started immediately. For boot start without login:
  `loginctl enable-linger $USER`. `--system` installs a system unit
  instead (run with sudo).
- **macOS**: a launchd LaunchAgent
  (`~/Library/LaunchAgents/org.raydriver.daemon.plist`), loaded at
  login and kept alive; logs go to `~/Library/Logs/raydriver-daemon.log`.
- **Windows**: a scheduled task (`raydriver-daemon`) running at
  logon, using `pythonw.exe` when available so no console window
  stays open.

`raydriver daemon uninstall` stops and removes it;
`raydriver daemon status` shows the service state plus a live
probe of the daemon (version, auth mode).

Manual systemd example (what `install` writes):

```ini
[Unit]
Description=raydriver device daemon
After=network-online.target

[Service]
ExecStart=/usr/bin/python3 -m raydriver daemon --config /etc/raydriver/daemon.json
Restart=on-failure
RestartSec=2

[Install]
WantedBy=default.target
```

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

## Debug commands

```sh
raydriver scan                    # local serial ports (vid:pid, product)
raydriver scan --network          # also browse for daemons via mDNS
raydriver info /dev/ttyUSB0       # one serial port's details
raydriver info 192.168.1.50:8765  # a daemon: sessions, devices, cameras
raydriver list                    # configured devices + live connection state
raydriver connection              # Rayforge clients connected to the daemon
raydriver daemon status           # service state + daemon probe
```

Every command accepts `--json` for machine-readable output. Daemon
commands read the endpoint and credentials from the config
(`--config` to override, `--token`/`--tls-ca` to override
credentials). `info` also accepts `raydriver://host:port#token`
URLs. `connection` shows each client's address, authentication
state and per-session roles, plus the control lease holder — the
daemon-side view of "who is driving the machine right now".

## Security policy

The analyzer's bind policy applies unchanged: loopback may run
without auth; a non-loopback or wildcard bind refuses to open
without **both** a token and TLS; explicitly configured auth is
never dropped. Clients refuse to send a token over a non-TLS
connection, except to loopback endpoints (`127.0.0.1`, `::1`,
`localhost`), where the token never leaves the machine — so the
config `--setup` writes works as-is for same-machine use. For an
exposed listener, generate a certificate with:

```sh
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout daemon-key.pem -out daemon-cert.pem -days 365 \
  -subj /CN=raydriver \
  -addext subjectAltName=DNS:localhost,IP:127.0.0.1
```

Add the LAN hostname/IP to `subjectAltName` for remote clients,
and pin the certificate on the client (`tls_ca`). Keep the daemon
port firewalled to the LAN; there is no WAN/NAT traversal.

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
`raydriver://host:8765#token`. The token is the one in the daemon's
`daemon.json` (generated by `daemon --setup`). Any raydriver
consumer can use `raydriver.remote.client.create_session` the same
way.