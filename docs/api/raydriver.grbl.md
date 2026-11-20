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
cancel(emergency: bool = False) -> Any
```

Abort the running job: soft reset, queue cleanup and safety shutdown.

| Parameter   | Type           | Description |
| ----------- | -------------- | ----------- |
| `emergency` | `bool = False` |             |
| _Returns_   | `Any`          |             |

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

Infer the device's unit system ("metric"/"imperial") from the `$13` setting.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `disconnect()`

```python
disconnect() -> Any
```

Stop all background tasks and close the transport.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `execute_command()`

```python
execute_command(command: str) -> Any
```

Queue a command for asynchronous processing and await its response.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `command` | `str` |             |
| _Returns_ | `Any` |             |

### `execute_interactive_command()`

```python
execute_interactive_command(command: str) -> Any
```

Send a command and synchronously await its full response, blocking any other command from
interleaving.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `command` | `str` |             |
| _Returns_ | `Any` |             |

### `home()`

```python
home(axes: Optional[Sequence[str]] = None, active_wcs: str = 'G54') -> Any
```

Home all axes (`axes` is None) or the named axes (e.g. `["X"]`), then re-activate the active WCS via
the toggle workaround.

| Parameter    | Type                             | Description |
| ------------ | -------------------------------- | ----------- |
| `axes`       | `Optional[Sequence[str]] = None` |             |
| `active_wcs` | `str = 'G54'`                    |             |
| _Returns_    | `Any`                            |             |

### `jog()`

```python
jog(speed: float, deltas: Sequence[tuple[str, float]]) -> Any
```

Jog by relative deltas (mm) at `speed` (mm/min).

| Parameter | Type                          | Description |
| --------- | ----------------------------- | ----------- |
| `speed`   | `float`                       |             |
| `deltas`  | `Sequence[tuple[str, float]]` |             |
| _Returns_ | `Any`                         |             |

### `move_to()`

```python
move_to(speed: float, x: float, y: float) -> Any
```

Move to an absolute position (mm) at `speed` (mm/min).

| Parameter | Type    | Description |
| --------- | ------- | ----------- |
| `speed`   | `float` |             |
| `x`       | `float` |             |
| `y`       | `float` |             |
| _Returns_ | `Any`   |             |

### `probe_device()`

```python
probe_device() -> Any
```

Open the connection and start the background tasks. Run the device census: query `$I` and `$$`, and
return the structured result as a dict.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `read_parser_state()`

```python
read_parser_state() -> Any
```

Read the `$G` parser state to determine the active WCS.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `read_settings()`

```python
read_settings() -> Any
```

Read `$$` into `(key, raw_value)` pairs.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `read_wcs_offsets()`

```python
read_wcs_offsets() -> Any
```

Read `$#` WCS offsets, converted to mm when the device reports inches.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `run()`

```python
run(
    gcode: str,
    op_line_map: Optional[Mapping[int, int]] = None,
    op_estimates: Optional[Sequence[float]] = None,
    progress_callback: Optional[Any] = None,
) -> Any
```

Stream a G-code program using the character-counting protocol.

`op_line_map` maps G-code line indices to op indices for progress reporting; `op_estimates` holds
per-op estimated durations (seconds) used to scale per-line stall timeouts.

| Parameter           | Type                                 | Description |
| ------------------- | ------------------------------------ | ----------- |
| `gcode`             | `str`                                |             |
| `op_line_map`       | `Optional[Mapping[int, int]] = None` |             |
| `op_estimates`      | `Optional[Sequence[float]] = None`   |             |
| `progress_callback` | `Optional[Any] = None`               |             |
| _Returns_           | `Any`                                |             |

### `run_probe_cycle()`

```python
run_probe_cycle(axis_letter: str, max_travel: float, feed_rate: float) -> Any
```

Run a probe cycle (G38.2); returns the trigger position in mm or None on failure.

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

Execute a raw G-code string; GRBL realtime commands (?, ~, !) are sent via the control path instead
of the stream.

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

Set laser power for focus mode; `None` waits for pending jogs to finish and turns the laser off.

| Parameter | Type                     | Description |
| --------- | ------------------------ | ----------- |
| `power`   | `Optional[float] = None` |             |
| _Returns_ | `Any`                    |             |

### `set_hold()`

```python
set_hold(hold: bool = True) -> Any
```

Pause (feed hold) or resume the device.

| Parameter | Type          | Description |
| --------- | ------------- | ----------- |
| `hold`    | `bool = True` |             |
| _Returns_ | `Any`         |             |

### `set_power()`

```python
set_power(power: Optional[float] = None) -> Any
```

Set laser power. `power` is the absolute S value; `None` (or zero) turns the laser off.

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
update_dialect(dialect: dict) -> None
```

Swap the dialect command templates at runtime.

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| `dialect` | `dict` |             |
| _Returns_ | `None` |             |

### `with_transport()`

```python
with_transport(
    transport: transport.MockTransport,
    config: Optional[Union[dict, config.GrblSerialConfig, config.Esp3dConfig]] = None,
    dialect: Optional[dict] = None,
    event_callback: Optional[Callable[[core.SessionEvent], Any]] = None,
) -> GrblSession
```

Build a session around a \[`MockTransport`\] (for tests).

| Parameter        | Type                                                                        | Description |
| ---------------- | --------------------------------------------------------------------------- | ----------- |
| `transport`      | `transport.MockTransport`                                                   |             |
| `config`         | `Optional[Union[dict, config.GrblSerialConfig, config.Esp3dConfig]] = None` |             |
| `dialect`        | `Optional[dict] = None`                                                     |             |
| `event_callback` | `Optional[Callable[[core.SessionEvent], Any]] = None`                       |             |
| _Returns_        | `GrblSession`                                                               |             |

### `write_setting()`

```python
write_setting(key: str, value: str) -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `key`     | `str` |             |
| `value`   | `str` |             |
| _Returns_ | `Any` |             |
