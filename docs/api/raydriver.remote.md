---
title: raydriver.remote
sidebar_label: raydriver.remote
---

Remote sessions: a device-host daemon, a client proxy and the versioned wire codec that connects
them.

## Announcer

### `info`

```python
info: ServiceInfo
```

## ClientQueue

### `get()`

```python
get() -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `put()`

```python
put(frame: Any) -> None
```

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| `frame`   | `Any`  |             |
| _Returns_ | `None` |             |

## Daemon

### `config`

```python
config: DaemonConfig
```

### `start()`

```python
start() -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `stop()`

```python
stop() -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

## DaemonConfig

### `cameras`

```python
cameras: Any
```

### `host`

```python
host: str
```

### `keepalive_interval`

```python
keepalive_interval: float
```

### `keepalive_misses`

```python
keepalive_misses: int
```

### `lease_grace`

```python
lease_grace: float
```

### `mdns`

```python
mdns: bool
```

### `port`

```python
port: int
```

### `ring_size`

```python
ring_size: int
```

### `sessions`

```python
sessions: Any
```

### `tls_cert`

```python
tls_cert: Optional[str]
```

### `tls_key`

```python
tls_key: Optional[str]
```

### `token`

```python
token: Optional[str]
```

## RemoteSession

The proxy with the union of the sessions' method surfaces.

### `buffer_count`

```python
buffer_count: int
```

### `job_running`

```python
job_running: bool
```

### `metadata`

```python
metadata: Any
```

### `pending_commands`

```python
pending_commands: list[str]
```

### `protocol`

```python
protocol: Optional[str]
```

### `resource_uri`

```python
resource_uri: Optional[str]
```

### `rx_buffer_size`

```python
rx_buffer_size: int
```

### `state`

```python
state: types.DeviceState
```

### `aclose()`

```python
aclose() -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `attach()`

```python
attach() -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `cancel()`

```python
cancel(emergency: bool = False) -> Any
```

| Parameter   | Type           | Description |
| ----------- | -------------- | ----------- |
| `emergency` | `bool = False` |             |
| _Returns_   | `Any`          |             |

### `clear_alarm()`

```python
clear_alarm() -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `connect()`

```python
connect() -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `detect_unit_system()`

```python
detect_unit_system() -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `disconnect()`

```python
disconnect() -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `drop_connection()`

```python
drop_connection(message: Optional[str] = None) -> None
```

| Parameter | Type                   | Description |
| --------- | ---------------------- | ----------- |
| `message` | `Optional[str] = None` |             |
| _Returns_ | `None`                 |             |

### `execute_command()`

```python
execute_command(command: str) -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `command` | `str` |             |
| _Returns_ | `Any` |             |

### `execute_interactive_command()`

```python
execute_interactive_command(command: str) -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `command` | `str` |             |
| _Returns_ | `Any` |             |

### `home()`

```python
home(
    axes: Optional[Sequence[str]] = None,
    active_wcs: Optional[str] = None,
) -> Any
```

| Parameter    | Type                             | Description |
| ------------ | -------------------------------- | ----------- |
| `axes`       | `Optional[Sequence[str]] = None` |             |
| `active_wcs` | `Optional[str] = None`           |             |
| _Returns_    | `Any`                            |             |

### `jog()`

```python
jog(speed: float, deltas: Any) -> Any
```

| Parameter | Type    | Description |
| --------- | ------- | ----------- |
| `speed`   | `float` |             |
| `deltas`  | `Any`   |             |
| _Returns_ | `Any`   |             |

### `move_to()`

```python
move_to(speed: float, x: float, y: float) -> Any
```

| Parameter | Type    | Description |
| --------- | ------- | ----------- |
| `speed`   | `float` |             |
| `x`       | `float` |             |
| `y`       | `float` |             |
| _Returns_ | `Any`   |             |

### `read_parser_state()`

```python
read_parser_state() -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `read_settings()`

```python
read_settings() -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `read_wcs_offsets()`

```python
read_wcs_offsets() -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `run()`

```python
run(
    gcode: str,
    op_line_map: Optional[Any] = None,
    op_estimates: Optional[Sequence[float]] = None,
    progress_callback: Optional[Any] = None,
) -> Any
```

| Parameter           | Type                               | Description |
| ------------------- | ---------------------------------- | ----------- |
| `gcode`             | `str`                              |             |
| `op_line_map`       | `Optional[Any] = None`             |             |
| `op_estimates`      | `Optional[Sequence[float]] = None` |             |
| `progress_callback` | `Optional[Any] = None`             |             |
| _Returns_           | `Any`                              |             |

### `run_probe_cycle()`

```python
run_probe_cycle(axis_letter: str, max_travel: float, feed_rate: float) -> Any
```

| Parameter     | Type    | Description |
| ------------- | ------- | ----------- |
| `axis_letter` | `str`   |             |
| `max_travel`  | `float` |             |
| `feed_rate`   | `float` |             |
| _Returns_     | `Any`   |             |

### `run_raw()`

```python
run_raw(machine_code: str) -> Any
```

| Parameter      | Type  | Description |
| -------------- | ----- | ----------- |
| `machine_code` | `str` |             |
| _Returns_      | `Any` |             |

### `select_tool()`

```python
select_tool(tool_number: int) -> Any
```

| Parameter     | Type  | Description |
| ------------- | ----- | ----------- |
| `tool_number` | `int` |             |
| _Returns_     | `Any` |             |

### `set_focus_power()`

```python
set_focus_power(power: Optional[float] = None) -> Any
```

| Parameter | Type                     | Description |
| --------- | ------------------------ | ----------- |
| `power`   | `Optional[float] = None` |             |
| _Returns_ | `Any`                    |             |

### `set_hold()`

```python
set_hold(hold: bool = True) -> Any
```

| Parameter | Type          | Description |
| --------- | ------------- | ----------- |
| `hold`    | `bool = True` |             |
| _Returns_ | `Any`         |             |

### `set_power()`

```python
set_power(power: Optional[float] = None) -> Any
```

| Parameter | Type                     | Description |
| --------- | ------------------------ | ----------- |
| `power`   | `Optional[float] = None` |             |
| _Returns_ | `Any`                    |             |

### `set_wcs_offset()`

```python
set_wcs_offset(wcs_slot: str, x: float, y: float, z: Optional[float]) -> Any
```

| Parameter  | Type              | Description |
| ---------- | ----------------- | ----------- |
| `wcs_slot` | `str`             |             |
| `x`        | `float`           |             |
| `y`        | `float`           |             |
| `z`        | `Optional[float]` |             |
| _Returns_  | `Any`             |             |

### `update_dialect()`

```python
update_dialect(dialect: Any) -> None
```

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| `dialect` | `Any`  |             |
| _Returns_ | `None` |             |

### `write_setting()`

```python
write_setting(key: str, value: str) -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `key`     | `str` |             |
| `value`   | `str` |             |
| _Returns_ | `Any` |             |

## ServiceInfo

### `port`

```python
port: int
```

## Functions

### `_announce_address()`

```python
_announce_address(config: DaemonConfig) -> str
```

| Parameter | Type           | Description |
| --------- | -------------- | ----------- |
| `config`  | `DaemonConfig` |             |
| _Returns_ | `str`          |             |

### `_service_name()`

```python
_service_name() -> str
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `str` |             |

### `_service_name_for()`

```python
_service_name_for(host: str) -> str
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `host`    | `str` |             |
| _Returns_ | `str` |             |

### `announce()`

```python
announce(config: DaemonConfig, port: int, name: Optional[str] = None) -> Any
```

| Parameter | Type                   | Description |
| --------- | ---------------------- | ----------- |
| `config`  | `DaemonConfig`         |             |
| `port`    | `int`                  |             |
| `name`    | `Optional[str] = None` |             |
| _Returns_ | `Any`                  |             |

### `browse_daemons()`

```python
browse_daemons(timeout: float = 3.0) -> Any
```

| Parameter | Type          | Description |
| --------- | ------------- | ----------- |
| `timeout` | `float = 3.0` |             |
| _Returns_ | `Any`         |             |

### `create_session()`

```python
create_session(
    protocol_name: str,
    config: Any,
    dialect: Optional[Any],
    event_callback: Optional[Any],
) -> Any
```

| Parameter        | Type            | Description |
| ---------------- | --------------- | ----------- |
| `protocol_name`  | `str`           |             |
| `config`         | `Any`           |             |
| `dialect`        | `Optional[Any]` |             |
| `event_callback` | `Optional[Any]` |             |
| _Returns_        | `Any`           |             |

### `daemon_rpc()`

```python
daemon_rpc(
    host: str,
    port: int,
    token: Optional[str],
    tls_ca: Optional[str],
    requests: Any,
) -> Any
```

| Parameter  | Type            | Description |
| ---------- | --------------- | ----------- |
| `host`     | `str`           |             |
| `port`     | `int`           |             |
| `token`    | `Optional[str]` |             |
| `tls_ca`   | `Optional[str]` |             |
| `requests` | `Any`           |             |
| _Returns_  | `Any`           |             |

### `decode_event()`

```python
decode_event(payload: Any) -> Optional[core.SessionEvent]
```

| Parameter | Type                          | Description |
| --------- | ----------------------------- | ----------- |
| `payload` | `Any`                         |             |
| _Returns_ | `Optional[core.SessionEvent]` |             |

### `decode_frame()`

```python
decode_frame(payload: Sequence[int], max_bytes: int = 16777216) -> Any
```

| Parameter   | Type             | Description |
| ----------- | ---------------- | ----------- |
| `payload`   | `Sequence[int]`  |             |
| `max_bytes` | `int = 16777216` |             |
| _Returns_   | `Any`            |             |

### `decode_state()`

```python
decode_state(payload: Any) -> types.DeviceState
```

| Parameter | Type                | Description |
| --------- | ------------------- | ----------- |
| `payload` | `Any`               |             |
| _Returns_ | `types.DeviceState` |             |

### `encode_event()`

```python
encode_event(event: core.SessionEvent) -> Any
```

| Parameter | Type                | Description |
| --------- | ------------------- | ----------- |
| `event`   | `core.SessionEvent` |             |
| _Returns_ | `Any`               |             |

### `encode_frame()`

```python
encode_frame(message: Any, max_bytes: int = 16777216) -> bytes
```

| Parameter   | Type             | Description |
| ----------- | ---------------- | ----------- |
| `message`   | `Any`            |             |
| `max_bytes` | `int = 16777216` |             |
| _Returns_   | `bytes`          |             |

### `encode_state()`

```python
encode_state(state: types.DeviceState) -> Any
```

| Parameter | Type                | Description |
| --------- | ------------------- | ----------- |
| `state`   | `types.DeviceState` |             |
| _Returns_ | `Any`               |             |

### `error_response()`

```python
error_response(msg_id: int, code: str, message: str) -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `msg_id`  | `int` |             |
| `code`    | `str` |             |
| `message` | `str` |             |
| _Returns_ | `Any` |             |

### `event_frame()`

```python
event_frame(session_id: str, seq: int, event: Any) -> Any
```

| Parameter    | Type  | Description |
| ------------ | ----- | ----------- |
| `session_id` | `str` |             |
| `seq`        | `int` |             |
| `event`      | `Any` |             |
| _Returns_    | `Any` |             |

### `list_devices()`

```python
list_devices(endpoint: Any) -> Any
```

| Parameter  | Type  | Description |
| ---------- | ----- | ----------- |
| `endpoint` | `Any` |             |
| _Returns_  | `Any` |             |

### `list_protocols()`

```python
list_protocols(endpoint: Any) -> Any
```

| Parameter  | Type  | Description |
| ---------- | ----- | ----------- |
| `endpoint` | `Any` |             |
| _Returns_  | `Any` |             |

### `open_session()`

```python
open_session(
    endpoint: Any,
    device: Optional[str] = None,
    session_id: Optional[str] = None,
    config: Optional[Any] = None,
    dialect: Optional[Any] = None,
    event_callback: Optional[Any] = None,
) -> RemoteSession
```

| Parameter        | Type                   | Description |
| ---------------- | ---------------------- | ----------- |
| `endpoint`       | `Any`                  |             |
| `device`         | `Optional[str] = None` |             |
| `session_id`     | `Optional[str] = None` |             |
| `config`         | `Optional[Any] = None` |             |
| `dialect`        | `Optional[Any] = None` |             |
| `event_callback` | `Optional[Any] = None` |             |
| _Returns_        | `RemoteSession`        |             |

### `request()`

```python
request(msg_id: int, type_: str, payload: Optional[Any] = None) -> Any
```

| Parameter | Type                   | Description |
| --------- | ---------------------- | ----------- |
| `msg_id`  | `int`                  |             |
| `type_`   | `str`                  |             |
| `payload` | `Optional[Any] = None` |             |
| _Returns_ | `Any`                  |             |

### `response()`

```python
response(msg_id: int, payload: Optional[Any] = None) -> Any
```

| Parameter | Type                   | Description |
| --------- | ---------------------- | ----------- |
| `msg_id`  | `int`                  |             |
| `payload` | `Optional[Any] = None` |             |
| _Returns_ | `Any`                  |             |

### `snapshot()`

```python
snapshot(
    host: str,
    port: int,
    token: Optional[str] = None,
    tls_ca: Optional[str] = None,
) -> Any
```

| Parameter | Type                   | Description |
| --------- | ---------------------- | ----------- |
| `host`    | `str`                  |             |
| `port`    | `int`                  |             |
| `token`   | `Optional[str] = None` |             |
| `tls_ca`  | `Optional[str] = None` |             |
| _Returns_ | `Any`                  |             |

### `welcome_payload()`

```python
welcome_payload(raydriver_version: str, auth_required: bool) -> Any
```

| Parameter           | Type   | Description |
| ------------------- | ------ | ----------- |
| `raydriver_version` | `str`  |             |
| `auth_required`     | `bool` |             |
| _Returns_           | `Any`  |             |

### `welcome_probe()`

```python
welcome_probe(host: str, port: int) -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `host`    | `str` |             |
| `port`    | `int` |             |
| _Returns_ | `Any` |             |
