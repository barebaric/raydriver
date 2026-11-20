---
title: raydriver.remote.client
sidebar_label: raydriver.remote.client
---

## Constants

### DEFAULT_PORT

`int`

### REMOTE_KEYS

`list[str]`

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

## Functions

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
