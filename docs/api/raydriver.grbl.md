---
title: raydriver.grbl
sidebar_label: raydriver.grbl
---

GRBL device drivers implemented in Rust.

The central type is GrblSession: it owns the connection lifecycle (handshake, status polling,
reconnect), interactive commands, and job streaming with character-counting flow control, stall
detection and deadlock recovery. GrblSimpleSession (raydriver.grbl.simple) is the minimal
alternative: strict ping-pong, for hardware where the full driver misbehaves. Dialects remain data
owned by Rayforge: resolved command templates are passed into the session.

## GrblSession

A GRBL device session: connection lifecycle, streaming and interactive commands, entirely in Rust.

All async methods return asyncio futures; a typed `SessionEvent` is delivered to
`event_callback(event)` on the event loop thread.

### `buffer_count`

```python
buffer_count: int
```

### `job_running`

```python
job_running: bool
```

### `pending_commands`

```python
pending_commands: list[str]
```

Commands awaiting their 'ok' acknowledgement.

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

Current snapshot of the device state.

### `cancel()`

```python
cancel(emergency: bool = False) -> Awaitable[None]
```

Cancel the running job.

| Parameter   | Type              | Description |
| ----------- | ----------------- | ----------- |
| `emergency` | `bool = False`    |             |
| _Returns_   | `Awaitable[None]` |             |

### `clear_alarm()`

```python
clear_alarm() -> Awaitable[None]
```

Clear a device alarm.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `connect()`

```python
connect() -> Awaitable[None]
```

Open the connection and keep it up until `disconnect`.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `detect_unit_system()`

```python
detect_unit_system() -> Awaitable[Optional[str]]
```

Detect the device's unit system (`"metric"` or `"imperial"`), or `None` when unknown.

| Parameter | Type                       | Description |
| --------- | -------------------------- | ----------- |
| _Returns_ | `Awaitable[Optional[str]]` |             |

### `disconnect()`

```python
disconnect() -> Awaitable[None]
```

Stop all background tasks and close the connection.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `execute_command()`

```python
execute_command(command: str) -> Awaitable[list[str]]
```

Send one command and return its response lines.

| Parameter | Type                   | Description |
| --------- | ---------------------- | ----------- |
| `command` | `str`                  |             |
| _Returns_ | `Awaitable[list[str]]` |             |

### `execute_interactive_command()`

```python
execute_interactive_command(command: str) -> Awaitable[list[str]]
```

Send one interactive command and return its response lines.

| Parameter | Type                   | Description |
| --------- | ---------------------- | ----------- |
| `command` | `str`                  |             |
| _Returns_ | `Awaitable[list[str]]` |             |

### `home()`

```python
home(
    axes: Optional[Sequence[str]] = None,
    active_wcs: Optional[str] = None,
) -> Awaitable[None]
```

Home all axes or the given axes.

| Parameter    | Type                             | Description |
| ------------ | -------------------------------- | ----------- |
| `axes`       | `Optional[Sequence[str]] = None` |             |
| `active_wcs` | `Optional[str] = None`           |             |
| _Returns_    | `Awaitable[None]`                |             |

### `jog()`

```python
jog(
    speed: float,
    deltas: Optional[Sequence[tuple[str, float]]] = None,
) -> Awaitable[None]
```

Relative jog by per-axis distances.

| Parameter | Type                                           | Description |
| --------- | ---------------------------------------------- | ----------- |
| `speed`   | `float`                                        |             |
| `deltas`  | `Optional[Sequence[tuple[str, float]]] = None` |             |
| _Returns_ | `Awaitable[None]`                              |             |

### `move_to()`

```python
move_to(
    speed: float,
    x: float,
    y: float,
    z: Optional[float] = None,
) -> Awaitable[None]
```

Absolute move in the machine's current coordinate frame.

| Parameter | Type                     | Description |
| --------- | ------------------------ | ----------- |
| `speed`   | `float`                  |             |
| `x`       | `float`                  |             |
| `y`       | `float`                  |             |
| `z`       | `Optional[float] = None` |             |
| _Returns_ | `Awaitable[None]`        |             |

### `probe_device()`

```python
probe_device() -> Awaitable[census.GrblDeviceInfo]
```

Open the connection and start the background tasks. Run the device census: query `$I` and `$$`, and
return the structured result as a dict.

| Parameter | Type                               | Description |
| --------- | ---------------------------------- | ----------- |
| _Returns_ | `Awaitable[census.GrblDeviceInfo]` |             |

### `read_parser_state()`

```python
read_parser_state() -> Awaitable[Optional[str]]
```

Read the G-code parser state.

| Parameter | Type                       | Description |
| --------- | -------------------------- | ----------- |
| _Returns_ | `Awaitable[Optional[str]]` |             |

### `read_settings()`

```python
read_settings() -> Awaitable[list[tuple[str, str]]]
```

Read the device's firmware settings.

| Parameter | Type                               | Description |
| --------- | ---------------------------------- | ----------- |
| _Returns_ | `Awaitable[list[tuple[str, str]]]` |             |

### `read_wcs_offsets()`

```python
read_wcs_offsets() -> Awaitable[dict[str, tuple[float, float, float]]]
```

Read the work-coordinate-system offsets.

| Parameter | Type                                               | Description |
| --------- | -------------------------------------------------- | ----------- |
| _Returns_ | `Awaitable[dict[str, tuple[float, float, float]]]` |             |

### `run()`

```python
run(
    gcode: str,
    op_line_map: Optional[Mapping[int, int]] = None,
    op_estimates: Optional[Sequence[float]] = None,
    progress_callback: Optional[Callable[[int], object]] = None,
) -> Awaitable[None]
```

Stream a G-code job.

| Parameter           | Type                                       | Description |
| ------------------- | ------------------------------------------ | ----------- |
| `gcode`             | `str`                                      |             |
| `op_line_map`       | `Optional[Mapping[int, int]] = None`       |             |
| `op_estimates`      | `Optional[Sequence[float]] = None`         |             |
| `progress_callback` | `Optional[Callable[[int], object]] = None` |             |
| _Returns_           | `Awaitable[None]`                          |             |

### `run_probe_cycle()`

```python
run_probe_cycle(
    axis_letter: str,
    max_travel: float,
    feed_rate: float,
) -> Awaitable[Optional[tuple[float, float, float]]]
```

Run a probe cycle; `None` = no trigger reported.

| Parameter     | Type                                              | Description |
| ------------- | ------------------------------------------------- | ----------- |
| `axis_letter` | `str`                                             |             |
| `max_travel`  | `float`                                           |             |
| `feed_rate`   | `float`                                           |             |
| _Returns_     | `Awaitable[Optional[tuple[float, float, float]]]` |             |

### `run_raw()`

```python
run_raw(machine_code: str) -> Awaitable[None]
```

Execute pre-rendered machine code.

| Parameter      | Type              | Description |
| -------------- | ----------------- | ----------- |
| `machine_code` | `str`             |             |
| _Returns_      | `Awaitable[None]` |             |

### `select_tool()`

```python
select_tool(tool_number: int) -> Awaitable[None]
```

Select a tool.

| Parameter     | Type              | Description |
| ------------- | ----------------- | ----------- |
| `tool_number` | `int`             |             |
| _Returns_     | `Awaitable[None]` |             |

### `set_focus_power()`

```python
set_focus_power(power: Optional[float] = None) -> Awaitable[None]
```

Set the laser power used while focusing.

| Parameter | Type                     | Description |
| --------- | ------------------------ | ----------- |
| `power`   | `Optional[float] = None` |             |
| _Returns_ | `Awaitable[None]`        |             |

### `set_hold()`

```python
set_hold(hold: bool = True) -> Awaitable[None]
```

Pause or resume the running job.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `hold`    | `bool = True`     |             |
| _Returns_ | `Awaitable[None]` |             |

### `set_power()`

```python
set_power(power: Optional[float] = None) -> Awaitable[None]
```

Set the laser/spindle power; `None` disables the tool.

| Parameter | Type                     | Description |
| --------- | ------------------------ | ----------- |
| `power`   | `Optional[float] = None` |             |
| _Returns_ | `Awaitable[None]`        |             |

### `set_wcs_offset()`

```python
set_wcs_offset(
    wcs_slot: str,
    x: float,
    y: float,
    z: Optional[float] = None,
) -> Awaitable[None]
```

Write a work-coordinate-system offset.

| Parameter  | Type                     | Description |
| ---------- | ------------------------ | ----------- |
| `wcs_slot` | `str`                    |             |
| `x`        | `float`                  |             |
| `y`        | `float`                  |             |
| `z`        | `Optional[float] = None` |             |
| _Returns_  | `Awaitable[None]`        |             |

### `update_dialect()`

```python
update_dialect(dialect: dict) -> None
```

Replace the command templates.

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| `dialect` | `dict` |             |
| _Returns_ | `None` |             |

### `with_transport()`

```python
with_transport(
    transport: transport.MockTransport,
    config: Optional[Union[dict, config.GrblSerialConfig, config.GrblTelnetConfig, config.GrblBluetoothConfig, config.Esp3dConfig]] = None,
    dialect: Optional[dict] = None,
    event_callback: Optional[Callable[[core.SessionEvent], object]] = None,
) -> GrblSession
```

Build a session around a \[`MockTransport`\] (for tests).

| Parameter        | Type                                                                                                                             | Description |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------- | ----------- |
| `transport`      | `transport.MockTransport`                                                                                                        |             |
| `config`         | `Optional[Union[dict, config.GrblSerialConfig, config.GrblTelnetConfig, config.GrblBluetoothConfig, config.Esp3dConfig]] = None` |             |
| `dialect`        | `Optional[dict] = None`                                                                                                          |             |
| `event_callback` | `Optional[Callable[[core.SessionEvent], object]] = None`                                                                         |             |
| _Returns_        | `GrblSession`                                                                                                                    |             |

### `write_setting()`

```python
write_setting(key: str, value: str) -> Awaitable[None]
```

Write one firmware setting.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `key`     | `str`             |             |
| `value`   | `str`             |             |
| _Returns_ | `Awaitable[None]` |             |
