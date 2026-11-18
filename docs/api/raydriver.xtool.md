---
title: raydriver.xtool
sidebar_label: raydriver.xtool
---

xTool device drivers implemented in Rust. XtoolSession supports D-series G-code jobs over the HTTP
API. M2Session is available at raydriver.xtool.m2 for WS-V2 monitoring and control; it does not
upload G-code jobs. Its untrusted-certificate option is explicit and disabled by default.

## XtoolSession

An xTool device session: connection lifecycle, interactive commands and job execution over the
proprietary WiFi API, entirely in Rust.

All async methods return asyncio futures; a typed `SessionEvent` is delivered to
`event_callback(event)` on the event loop thread.

### `is_job_running`

```python
is_job_running: bool
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

Stop the running job and send the safety-off commands.

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

### `disconnect()`

```python
disconnect() -> Any
```

Stop all background tasks and close the connection.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `execute_block()`

```python
execute_block(block: str) -> Any
```

Execute a multi-line block of G-code via POST /cmd.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `block`   | `str` |             |
| _Returns_ | `Any` |             |

### `execute_command()`

```python
execute_command(command: str) -> Any
```

Queue a single G-code line without failing on device-side rejections; the command status carries the
error.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `command` | `str` |             |
| _Returns_ | `Any` |             |

### `execute_interactive_command()`

```python
execute_interactive_command(command: str) -> Any
```

Send a single G-code line and await the device's HTTP acknowledgement. The protocol has no per-line
response text, so the result is always empty.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `command` | `str` |             |
| _Returns_ | `Any` |             |

### `home()`

```python
home() -> Any
```

Run the homing cycle and await the machine going idle again.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `jog()`

```python
jog(speed: float, deltas: Optional[Sequence[tuple[str, float]]] = None) -> Any
```

Jog by relative deltas (mm) at `speed` (mm/min).

| Parameter | Type                                           | Description |
| --------- | ---------------------------------------------- | ----------- |
| `speed`   | `float`                                        |             |
| `deltas`  | `Optional[Sequence[tuple[str, float]]] = None` |             |
| _Returns_ | `Any`                                          |             |

### `move_to()`

```python
move_to(
    x: float,
    y: float,
    z: Optional[float] = None,
    speed: Optional[float] = None,
) -> Any
```

Move to an absolute position (mm) at `speed` (mm/min).

| Parameter | Type                     | Description |
| --------- | ------------------------ | ----------- |
| `x`       | `float`                  |             |
| `y`       | `float`                  |             |
| `z`       | `Optional[float] = None` |             |
| `speed`   | `Optional[float] = None` |             |
| _Returns_ | `Any`                    |             |

### `run()`

```python
run(gcode: str) -> Any
```

Upload a G-code job, start it and await the machine's job-state events until it reports idle again.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `gcode`   | `str` |             |
| _Returns_ | `Any` |             |

### `run_raw()`

```python
run_raw(machine_code: str) -> Any
```

Upload a raw G-code string, start it and await completion.

| Parameter      | Type  | Description |
| -------------- | ----- | ----------- |
| `machine_code` | `str` |             |
| _Returns_      | `Any` |             |

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

Set laser power. `power` is the absolute S value; `None` turns the laser off.

| Parameter | Type                     | Description |
| --------- | ------------------------ | ----------- |
| `power`   | `Optional[float] = None` |             |
| _Returns_ | `Any`                    |             |

### `update_dialect()`

```python
update_dialect(dialect: dict) -> None
```

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| `dialect` | `dict` |             |
| _Returns_ | `None` |             |
