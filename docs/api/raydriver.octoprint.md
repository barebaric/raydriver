---
title: raydriver.octoprint
sidebar_label: raydriver.octoprint
---

OctoPrint server support implemented in Rust. OctoPrintSession submits G-code jobs over the server's
REST API and tracks printer state via its SockJS WebSocket (with REST polling fallback).

## OctoPrintSession

An OctoPrint server session: job submission and live printer state over the REST API and SockJS
WebSocket.

All async methods return asyncio futures; a typed `SessionEvent` is delivered to
`event_callback(event)` on the event loop thread. Operations the OctoPrint API cannot perform (e.g.
writing firmware settings) raise `NotImplementedError`.

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

Current snapshot of the printer state.

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

Clear the printer's error state (M999) and reset locally.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `connect()`

```python
connect() -> Awaitable[None]
```

Open the connection and start the background tasks.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `detect_unit_system()`

```python
detect_unit_system() -> Awaitable[Optional[str]]
```

The unit system the dialect works in; the server API exposes no unit configuration, so this is
always `None`.

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
execute_command(command: str) -> Awaitable[None]
```

Send a single command without awaiting further state.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `command` | `str`             |             |
| _Returns_ | `Awaitable[None]` |             |

### `execute_interactive_command()`

```python
execute_interactive_command(command: str) -> Awaitable[list[str]]
```

Send a single command and await the server's HTTP acknowledgement. The API exposes no per-line
response text, so the result is always empty.

| Parameter | Type                   | Description |
| --------- | ---------------------- | ----------- |
| `command` | `str`                  |             |
| _Returns_ | `Awaitable[list[str]]` |             |

### `home()`

```python
home(axes: Optional[Sequence[str]] = None) -> Awaitable[None]
```

Home all axes (`axes` is None) or the named axes.

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
move_to(
    x: float,
    y: float,
    z: Optional[float] = None,
    speed: Optional[float] = None,
) -> Awaitable[None]
```

Move to an absolute position (mm); `speed` in mm/min.

| Parameter | Type                     | Description |
| --------- | ------------------------ | ----------- |
| `x`       | `float`                  |             |
| `y`       | `float`                  |             |
| `z`       | `Optional[float] = None` |             |
| `speed`   | `Optional[float] = None` |             |
| _Returns_ | `Awaitable[None]`        |             |

### `probe_device()`

```python
probe_device() -> Awaitable[probe.OctoPrintDeviceInfo]
```

Run the device census over the session: the parsed `/api/version` and `/api/printer` responses,
interpreted into the structured census dict. Local and remote sessions probe identically.

| Parameter | Type                                   | Description |
| --------- | -------------------------------------- | ----------- |
| _Returns_ | `Awaitable[probe.OctoPrintDeviceInfo]` |             |

### `probe_info()`

```python
probe_info() -> Awaitable[list[dict[str, object]]]
```

Fetch the census payloads (the parsed `/api/version` and `/api/printer` responses) over the session,
so local and remote probing use the same code path.

| Parameter | Type                                 | Description |
| --------- | ------------------------------------ | ----------- |
| _Returns_ | `Awaitable[list[dict[str, object]]]` |             |

### `read_parser_state()`

```python
read_parser_state() -> Awaitable[Optional[str]]
```

Read the G-code parser state; always `None` (the server API exposes none).

| Parameter | Type                       | Description |
| --------- | -------------------------- | ----------- |
| _Returns_ | `Awaitable[Optional[str]]` |             |

### `read_settings()`

```python
read_settings() -> Awaitable[list[tuple[str, str]]]
```

Read the firmware settings; always empty (the server API exposes none).

| Parameter | Type                               | Description |
| --------- | ---------------------------------- | ----------- |
| _Returns_ | `Awaitable[list[tuple[str, str]]]` |             |

### `read_wcs_offsets()`

```python
read_wcs_offsets() -> Awaitable[dict[str, tuple[float, float, float]]]
```

Read the WCS offsets; always empty (the server API exposes none).

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

Upload a G-code string, start it and await the server's terminal print event. Every op index in
*op_line_map* is reported to *progress_callback* when the job starts (the protocol carries no per-op
progress).

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
) -> Awaitable[Optional[str]]
```

Run a probe cycle (G38.2); OctoPrint does not report probe results, so the result is always `None`.

| Parameter     | Type                       | Description |
| ------------- | -------------------------- | ----------- |
| `axis_letter` | `str`                      |             |
| `max_travel`  | `float`                    |             |
| `feed_rate`   | `float`                    |             |
| _Returns_     | `Awaitable[Optional[str]]` |             |

### `run_raw()`

```python
run_raw(machine_code: str) -> Awaitable[None]
```

Send a raw G-code string as printer commands.

| Parameter      | Type              | Description |
| -------------- | ----------------- | ----------- |
| `machine_code` | `str`             |             |
| _Returns_      | `Awaitable[None]` |             |

### `select_tool()`

```python
select_tool(tool_number: int) -> Awaitable[None]
```

Select a tool by sending its tool-change command.

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

### `set_hold()`

```python
set_hold(hold: bool = True) -> Awaitable[None]
```

Pause (true) or resume (false) the running job.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `hold`    | `bool = True`     |             |
| _Returns_ | `Awaitable[None]` |             |

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

Set a work coordinate offset via a G10 L2 command. Raises `ValueError` for slots the server model
does not know.

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

Accept dialect command templates; commands are sent as structured JSON, so nothing is stored.

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| `dialect` | `dict` |             |
| _Returns_ | `None` |             |

### `write_setting()`

```python
write_setting(key: str, value: str) -> Awaitable[None]
```

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `key`     | `str`             |             |
| `value`   | `str`             |             |
| _Returns_ | `Awaitable[None]` |             |
