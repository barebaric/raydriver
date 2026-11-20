---
title: raydriver.xtool.m2
sidebar_label: raydriver.xtool.m2
---

M2 WS-V2 driver.

## M2Session

Monitor and control an xTool M2 over its WS-V2 TLS service. This session does not upload G-code
jobs. `start` controls a job that is already staged on the device, such as a job sent by xTool
Studio.

### `is_job_running`

```python
is_job_running: bool
```

Whether the state currently indicates an active or held job.

### `resource_uri`

```python
resource_uri: Optional[str]
```

WebSocket URI of the configured device.

### `state`

```python
state: types.DeviceState
```

Current snapshot of the machine state.

### `cancel()`

```python
cancel() -> Awaitable[None]
```

Cancel the current job.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `connect()`

```python
connect() -> Awaitable[None]
```

Connect and start monitoring the M2.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `disconnect()`

```python
disconnect() -> Awaitable[None]
```

Stop monitoring and close the connection.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `home()`

```python
home() -> Awaitable[None]
```

Home the laser head.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `jog()`

```python
jog(
    dx: float = 0.0,
    dy: float = 0.0,
    dz: float = 0.0,
    speed: float = 18000.0,
) -> Awaitable[None]
```

Move by relative XYZ deltas in millimeters.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `dx`      | `float = 0.0`     |             |
| `dy`      | `float = 0.0`     |             |
| `dz`      | `float = 0.0`     |             |
| `speed`   | `float = 18000.0` |             |
| _Returns_ | `Awaitable[None]` |             |

### `move_to()`

```python
move_to(x: float, y: float, speed: float = 18000.0) -> Awaitable[None]
```

Move to an absolute XY coordinate in millimeters.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `x`       | `float`           |             |
| `y`       | `float`           |             |
| `speed`   | `float = 18000.0` |             |
| _Returns_ | `Awaitable[None]` |             |

### `set_hold()`

```python
set_hold(hold: bool = True) -> Awaitable[None]
```

Pause the current job, or resume it when `hold` is false.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `hold`    | `bool = True`     |             |
| _Returns_ | `Awaitable[None]` |             |

### `start()`

```python
start() -> Awaitable[None]
```

Start a job already staged on the device.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |
