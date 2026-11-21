---
title: raydriver.marlin
sidebar_label: raydriver.marlin
---

Marlin device session: strict ping-pong commands, M114 position polling. Dialects remain data owned
by Rayforge: resolved command templates are passed into the session.

## MarlinSession

A Marlin device session using ping-pong communication.

Sends one command at a time and waits for `ok` before proceeding; the device announces itself with a
`start` line and positions are polled with `M114`. Marlin has no feed-hold support — pausing is the
caller's concern.

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
cancel(emergency: bool = False) -> Awaitable[None]
```

Abort the running job: M410 quick stop and safety shutdown.

| Parameter   | Type              | Description |
| ----------- | ----------------- | ----------- |
| `emergency` | `bool = False`    |             |
| _Returns_   | `Awaitable[None]` |             |

### `clear_alarm()`

```python
clear_alarm() -> Awaitable[None]
```

Clear an alarm state with the dialect's clear command.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `connect()`

```python
connect() -> Awaitable[None]
```

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `detect_unit_system()`

```python
detect_unit_system() -> Awaitable[Optional[str]]
```

Query the device's active linear unit via `M149` ("metric"/"imperial"), or None when it cannot be
determined.

| Parameter | Type                       | Description |
| --------- | -------------------------- | ----------- |
| _Returns_ | `Awaitable[Optional[str]]` |             |

### `disconnect()`

```python
disconnect() -> Awaitable[None]
```

Stop all background tasks and close the transport.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `execute_command()`

```python
execute_command(command: str) -> Awaitable[list[str]]
```

Queue a command and await its response (identical to `execute_interactive_command`).

| Parameter | Type                   | Description |
| --------- | ---------------------- | ----------- |
| `command` | `str`                  |             |
| _Returns_ | `Awaitable[list[str]]` |             |

### `execute_interactive_command()`

```python
execute_interactive_command(command: str) -> Awaitable[list[str]]
```

Send a command and await its full response.

| Parameter | Type                   | Description |
| --------- | ---------------------- | ----------- |
| `command` | `str`                  |             |
| _Returns_ | `Awaitable[list[str]]` |             |

### `home()`

```python
home(axes: Optional[Sequence[str]] = None) -> Awaitable[None]
```

Home all axes (`axes` is None) or the named axes (e.g. `["X"]`).

| Parameter | Type                             | Description |
| --------- | -------------------------------- | ----------- |
| `axes`    | `Optional[Sequence[str]] = None` |             |
| _Returns_ | `Awaitable[None]`                |             |

### `jog()`

```python
jog(speed: float, deltas: Sequence[tuple[str, float]]) -> Awaitable[None]
```

Jog by relative deltas (mm) at `speed` (mm/min).

| Parameter | Type                          | Description |
| --------- | ----------------------------- | ----------- |
| `speed`   | `float`                       |             |
| `deltas`  | `Sequence[tuple[str, float]]` |             |
| _Returns_ | `Awaitable[None]`             |             |

### `move_to()`

```python
move_to(speed: float, x: float, y: float) -> Awaitable[None]
```

Move to an absolute position (mm) at `speed` (mm/min).

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `speed`   | `float`           |             |
| `x`       | `float`           |             |
| `y`       | `float`           |             |
| _Returns_ | `Awaitable[None]` |             |

### `probe_device()`

```python
probe_device(
    boot_lines: Optional[Sequence[str]] = None,
) -> Awaitable[probe.MarlinDeviceInfo]
```

Open the connection and start the background tasks. Run the device census: query M115, M211, M503
and M149, and return the structured result as a dict. *boot_lines* are the startup messages the
caller observed on the session, used to refine the firmware version.

| Parameter    | Type                                | Description |
| ------------ | ----------------------------------- | ----------- |
| `boot_lines` | `Optional[Sequence[str]] = None`    |             |
| _Returns_    | `Awaitable[probe.MarlinDeviceInfo]` |             |

### `run()`

```python
run(
    gcode: str,
    op_line_map: Optional[Mapping[int, int]] = None,
    progress_callback: Optional[Callable[[int], object]] = None,
) -> Awaitable[None]
```

Stream a G-code program using the ping-pong protocol.

`op_line_map` maps G-code line indices to op indices for progress reporting.

| Parameter           | Type                                       | Description |
| ------------------- | ------------------------------------------ | ----------- |
| `gcode`             | `str`                                      |             |
| `op_line_map`       | `Optional[Mapping[int, int]] = None`       |             |
| `progress_callback` | `Optional[Callable[[int], object]] = None` |             |
| _Returns_           | `Awaitable[None]`                          |             |

### `run_raw()`

```python
run_raw(machine_code: str) -> Awaitable[None]
```

Execute a raw G-code string line by line.

| Parameter      | Type              | Description |
| -------------- | ----------------- | ----------- |
| `machine_code` | `str`             |             |
| _Returns_      | `Awaitable[None]` |             |

### `select_tool()`

```python
select_tool(tool_number: int) -> Awaitable[None]
```

| Parameter     | Type              | Description |
| ------------- | ----------------- | ----------- |
| `tool_number` | `int`             |             |
| _Returns_     | `Awaitable[None]` |             |

### `set_focus_power()`

```python
set_focus_power(power: Optional[float] = None) -> Awaitable[None]
```

Set laser power for focus mode; `None` turns the laser off.

| Parameter | Type                     | Description |
| --------- | ------------------------ | ----------- |
| `power`   | `Optional[float] = None` |             |
| _Returns_ | `Awaitable[None]`        |             |

### `set_power()`

```python
set_power(power: Optional[float] = None) -> Awaitable[None]
```

Set laser power. `power` is the absolute S value; `None` (or zero) turns the laser off.

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
    z: Optional[float],
) -> Awaitable[None]
```

Set a WCS offset via the dialect's `set_wcs_offset` template.

| Parameter  | Type              | Description |
| ---------- | ----------------- | ----------- |
| `wcs_slot` | `str`             |             |
| `x`        | `float`           |             |
| `y`        | `float`           |             |
| `z`        | `Optional[float]` |             |
| _Returns_  | `Awaitable[None]` |             |

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
    config: Optional[Union[dict, config.MarlinConfig]] = None,
    dialect: Optional[dict] = None,
    event_callback: Optional[Callable[[core.SessionEvent], object]] = None,
) -> MarlinSession
```

Build a session around a \[`MockTransport`\] (for tests).

| Parameter        | Type                                                     | Description |
| ---------------- | -------------------------------------------------------- | ----------- |
| `transport`      | `transport.MockTransport`                                |             |
| `config`         | `Optional[Union[dict, config.MarlinConfig]] = None`      |             |
| `dialect`        | `Optional[dict] = None`                                  |             |
| `event_callback` | `Optional[Callable[[core.SessionEvent], object]] = None` |             |
| _Returns_        | `MarlinSession`                                          |             |
