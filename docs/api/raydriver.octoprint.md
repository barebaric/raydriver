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
cancel(emergency: bool = False) -> Any
```

Cancel the running job.

| Parameter   | Type           | Description |
| ----------- | -------------- | ----------- |
| `emergency` | `bool = False` |             |
| _Returns_   | `Any`          |             |

### `clear_alarm()`

```python
clear_alarm() -> Any
```

Clear the printer's error state (M999) and reset locally.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

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

The unit system the dialect works in; the server API exposes no unit configuration, so this is
always `None`.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `disconnect()`

```python
disconnect() -> Any
```

Stop all background tasks and close the connection.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `execute_command()`

```python
execute_command(command: str) -> Any
```

Send a single command without awaiting further state.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `command` | `str` |             |
| _Returns_ | `Any` |             |

### `execute_interactive_command()`

```python
execute_interactive_command(command: str) -> Any
```

Send a single command and await the server's HTTP acknowledgement. The API exposes no per-line
response text, so the result is always empty.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `command` | `str` |             |
| _Returns_ | `Any` |             |

### `home()`

```python
home(axes: Optional[Sequence[str]] = None) -> Any
```

Home all axes (`axes` is None) or the named axes.

| Parameter | Type                             | Description |
| --------- | -------------------------------- | ----------- |
| `axes`    | `Optional[Sequence[str]] = None` |             |
| _Returns_ | `Any`                            |             |

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
move_to(
    x: float,
    y: float,
    z: Optional[float] = None,
    speed: Optional[float] = None,
) -> Any
```

Move to an absolute position (mm); `speed` in mm/min.

| Parameter | Type                     | Description |
| --------- | ------------------------ | ----------- |
| `x`       | `float`                  |             |
| `y`       | `float`                  |             |
| `z`       | `Optional[float] = None` |             |
| `speed`   | `Optional[float] = None` |             |
| _Returns_ | `Any`                    |             |

### `probe_device()`

```python
probe_device() -> Any
```

Run the device census over the session: the parsed `/api/version` and `/api/printer` responses,
interpreted into the structured census dict. Local and remote sessions probe identically.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `probe_info()`

```python
probe_info() -> Any
```

Fetch the census payloads (the parsed `/api/version` and `/api/printer` responses) over the session,
so local and remote probing use the same code path.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `read_parser_state()`

```python
read_parser_state() -> Any
```

Read the G-code parser state; always `None` (the server API exposes none).

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `read_settings()`

```python
read_settings() -> Any
```

Read the firmware settings; always empty (the server API exposes none).

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `read_wcs_offsets()`

```python
read_wcs_offsets() -> Any
```

Read the WCS offsets; always empty (the server API exposes none).

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

Upload a G-code string, start it and await the server's terminal print event. Every op index in
*op_line_map* is reported to *progress_callback* when the job starts (the protocol carries no per-op
progress).

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

Run a probe cycle (G38.2); OctoPrint does not report probe results, so the result is always `None`.

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

Send a raw G-code string as printer commands.

| Parameter      | Type  | Description |
| -------------- | ----- | ----------- |
| `machine_code` | `str` |             |
| _Returns_      | `Any` |             |

### `select_tool()`

```python
select_tool(tool_number: int) -> Any
```

Select a tool by sending its tool-change command.

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

Pause (true) or resume (false) the running job.

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

Set a work coordinate offset via a G10 L2 command. Raises `ValueError` for slots the server model
does not know.

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

Accept dialect command templates; commands are sent as structured JSON, so nothing is stored.

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| `dialect` | `dict` |             |
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
