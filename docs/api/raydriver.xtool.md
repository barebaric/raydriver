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
cancel(emergency: bool = False) -> Awaitable[None]
```

Stop the running job and send the safety-off commands.

| Parameter   | Type              | Description |
| ----------- | ----------------- | ----------- |
| `emergency` | `bool = False`    |             |
| _Returns_   | `Awaitable[None]` |             |

### `connect()`

```python
connect() -> Awaitable[None]
```

Open the connection and start the background tasks.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `disconnect()`

```python
disconnect() -> Awaitable[None]
```

Stop all background tasks and close the connection.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `execute_block()`

```python
execute_block(block: str) -> Awaitable[None]
```

Execute a multi-line block of G-code via POST /cmd.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `block`   | `str`             |             |
| _Returns_ | `Awaitable[None]` |             |

### `execute_command()`

```python
execute_command(command: str) -> Awaitable[None]
```

Queue a single G-code line without failing on device-side rejections; the command status carries the
error.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `command` | `str`             |             |
| _Returns_ | `Awaitable[None]` |             |

### `execute_interactive_command()`

```python
execute_interactive_command(command: str) -> Awaitable[list[str]]
```

Send a single G-code line and await the device's HTTP acknowledgement. The protocol has no per-line
response text, so the result is always empty.

| Parameter | Type                   | Description |
| --------- | ---------------------- | ----------- |
| `command` | `str`                  |             |
| _Returns_ | `Awaitable[list[str]]` |             |

### `home()`

```python
home() -> Awaitable[None]
```

Run the homing cycle and await the machine going idle again.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `jog()`

```python
jog(
    speed: float,
    deltas: Optional[Sequence[tuple[str, float]]] = None,
) -> Awaitable[None]
```

Jog by relative deltas (mm) at `speed` (mm/min).

| Parameter | Type                                           | Description |
| --------- | ---------------------------------------------- | ----------- |
| `speed`   | `float`                                        |             |
| `deltas`  | `Optional[Sequence[tuple[str, float]]] = None` |             |
| _Returns_ | `Awaitable[None]`                              |             |

### `move_to()`

```python
move_to(
    x: float,
    y: float,
    z: Optional[float] = None,
    speed: Optional[float] = None,
) -> Awaitable[None]
```

Move to an absolute position (mm) at `speed` (mm/min).

| Parameter | Type                     | Description |
| --------- | ------------------------ | ----------- |
| `x`       | `float`                  |             |
| `y`       | `float`                  |             |
| `z`       | `Optional[float] = None` |             |
| `speed`   | `Optional[float] = None` |             |
| _Returns_ | `Awaitable[None]`        |             |

### `run()`

```python
run(gcode: str) -> Awaitable[None]
```

Upload a G-code job, start it and await the machine's job-state events until it reports idle again.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `gcode`   | `str`             |             |
| _Returns_ | `Awaitable[None]` |             |

### `run_raw()`

```python
run_raw(machine_code: str) -> Awaitable[None]
```

Upload a raw G-code string, start it and await completion.

| Parameter      | Type              | Description |
| -------------- | ----------------- | ----------- |
| `machine_code` | `str`             |             |
| _Returns_      | `Awaitable[None]` |             |

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

Set laser power. `power` is the absolute S value; `None` turns the laser off.

| Parameter | Type                     | Description |
| --------- | ------------------------ | ----------- |
| `power`   | `Optional[float] = None` |             |
| _Returns_ | `Awaitable[None]`        |             |

### `update_dialect()`

```python
update_dialect(dialect: dict) -> None
```

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| `dialect` | `dict` |             |
| _Returns_ | `None` |             |

## Functions

### `query_machine_type()`

```python
query_machine_type(
    host: str,
    port: int,
    timeout: float = 2.0,
) -> Awaitable[Optional[str]]
```

Queries the machine type an xTool device reports for itself.

GETs the machine-type endpoint at *host*:*port* and resolves to the reported type when it names an
xTool machine, and `None` on any failure.

| Parameter | Type                       | Description |
| --------- | -------------------------- | ----------- |
| `host`    | `str`                      |             |
| `port`    | `int`                      |             |
| `timeout` | `float = 2.0`              |             |
| _Returns_ | `Awaitable[Optional[str]]` |             |
