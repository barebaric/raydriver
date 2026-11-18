---
title: raydriver.grbl.simple
sidebar_label: raydriver.grbl.simple
---

The minimal ping-pong GRBL session: one line at a time, waiting for each ok/error acknowledgement.
For hardware where the full driver misbehaves.

## GrblSimpleSession

A minimal GRBL device session using ping-pong communication.

Sends one G-code line at a time and waits for `ok` before proceeding. No character-counting, no
buffer management, no deadlock detection — for hardware where the full
\[`GrblSession`\][raydriver.grbl.GrblSession] misbehaves.

All async methods return asyncio futures; a typed `SessionEvent` is delivered to
`event_callback(event)` on the event loop thread.

### `job_running`

```python
job_running: bool
```

### `resource_uri`

```python
resource_uri: Optional[str]
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

Abort the running job: soft reset, pending-command cleanup and safety shutdown.

| Parameter   | Type           | Description |
| ----------- | -------------- | ----------- |
| `emergency` | `bool = False` |             |
| _Returns_   | `Any`          |             |

### `connect()`

```python
connect() -> Any
```

Open the connection and start the background tasks.

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

Queue a command and await its response (identical to `execute_interactive_command` — the simple
protocol has no separate queue).

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `command` | `str` |             |
| _Returns_ | `Any` |             |

### `execute_interactive_command()`

```python
execute_interactive_command(command: str) -> Any
```

Send a command and await its full response.

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
    progress_callback: Optional[Any] = None,
) -> Any
```

Stream a G-code program using the ping-pong protocol: one line at a time, waiting for each `ok`.

`op_line_map` maps G-code line indices to op indices for progress reporting.

| Parameter           | Type                                 | Description |
| ------------------- | ------------------------------------ | ----------- |
| `gcode`             | `str`                                |             |
| `op_line_map`       | `Optional[Mapping[int, int]] = None` |             |
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

Set laser power for focus mode; `None` turns the laser off.

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
    config: Optional[Union[dict, config.SessionConfig]] = None,
    dialect: Optional[dict] = None,
    event_callback: Optional[Callable[[core.SessionEvent], Any]] = None,
) -> GrblSimpleSession
```

Build a session around a \[`MockTransport`\] (for tests).

| Parameter        | Type                                                  | Description |
| ---------------- | ----------------------------------------------------- | ----------- |
| `transport`      | `transport.MockTransport`                             |             |
| `config`         | `Optional[Union[dict, config.SessionConfig]] = None`  |             |
| `dialect`        | `Optional[dict] = None`                               |             |
| `event_callback` | `Optional[Callable[[core.SessionEvent], Any]] = None` |             |
| _Returns_        | `GrblSimpleSession`                                   |             |

### `write_setting()`

```python
write_setting(key: str, value: str) -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `key`     | `str` |             |
| `value`   | `str` |             |
| _Returns_ | `Any` |             |
