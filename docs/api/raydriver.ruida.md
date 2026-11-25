---
title: raydriver.ruida
sidebar_label: raydriver.ruida
---

Ruida laser controllers implemented in Rust. RuidaDriver speaks the binary rpascript protocol over
USB, UDP or TCP: it runs complete job scripts, executes live motion and job-control commands, polls
the controller status and reports the machine state through the uniform driver event surface. Job
compilation from Ops lives in the encoder; this session consumes rpascript text.

## GlueScript

The Ruida job authoring and compilation model.

Call the authoring methods to build a job, then `stage_gluescript()` compiles it into rpascript.
`stage_gluescript(lines)` also replays a persisted transcript.

### `gluescript`

```python
gluescript: list[str]
```

The canonical transcript lines.

### `job_complete`

```python
job_complete: bool
```

Whether `end_job()` finalized the job.

### `rpascript`

```python
rpascript: list[str]
```

The compiled rpascript lines.

### `air_assist_off()`

```python
air_assist_off() -> None
```

Disable air assist.

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| _Returns_ | `None` |             |

### `air_assist_on()`

```python
air_assist_on() -> None
```

Enable air assist.

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| _Returns_ | `None` |             |

### `comment()`

```python
comment(comments: Sequence[str]) -> None
```

Append comment lines to the job header.

| Parameter  | Type            | Description |
| ---------- | --------------- | ----------- |
| `comments` | `Sequence[str]` |             |
| _Returns_  | `None`          |             |

### `cut_speed()`

```python
cut_speed(speed: float) -> None
```

Save the cut speed for the next cut.

| Parameter | Type    | Description |
| --------- | ------- | ----------- |
| `speed`   | `float` |             |
| _Returns_ | `None`  |             |

### `cut_x_to()`

```python
cut_x_to(x: float) -> None
```

Cut to an absolute X position.

| Parameter | Type    | Description |
| --------- | ------- | ----------- |
| `x`       | `float` |             |
| _Returns_ | `None`  |             |

### `cut_xy_to()`

```python
cut_xy_to(x: float, y: float) -> None
```

Cut to an absolute XY position.

| Parameter | Type    | Description |
| --------- | ------- | ----------- |
| `x`       | `float` |             |
| `y`       | `float` |             |
| _Returns_ | `None`  |             |

### `cut_y_to()`

```python
cut_y_to(y: float) -> None
```

Cut to an absolute Y position.

| Parameter | Type    | Description |
| --------- | ------- | ----------- |
| `y`       | `float` |             |
| _Returns_ | `None`  |             |

### `declare_job()`

```python
declare_job(
    label: str,
    ref_point: str = 'MACHINE',
    abs_xy: Optional[Sequence[float]] = None,
    columns: int = 1,
    rows: int = 1,
    xstep: float = 0.0,
    ystep: float = 0.0,
) -> None
```

Declare a job.

| Parameter   | Type                               | Description |
| ----------- | ---------------------------------- | ----------- |
| `label`     | `str`                              |             |
| `ref_point` | `str = 'MACHINE'`                  |             |
| `abs_xy`    | `Optional[Sequence[float]] = None` |             |
| `columns`   | `int = 1`                          |             |
| `rows`      | `int = 1`                          |             |
| `xstep`     | `float = 0.0`                      |             |
| `ystep`     | `float = 0.0`                      |             |
| _Returns_   | `None`                             |             |

### `declare_layer()`

```python
declare_layer(
    label: str,
    color: str,
    mode: str = 'VECTOR',
    overscan: str = 'NONE',
    speed: float = 100.0,
    frequency: float = 20.0,
    min_power_1: float = 8.0,
    max_power_1: float = 70.0,
) -> None
```

Declare a layer.

| Parameter     | Type             | Description |
| ------------- | ---------------- | ----------- |
| `label`       | `str`            |             |
| `color`       | `str`            |             |
| `mode`        | `str = 'VECTOR'` |             |
| `overscan`    | `str = 'NONE'`   |             |
| `speed`       | `float = 100.0`  |             |
| `frequency`   | `float = 20.0`   |             |
| `min_power_1` | `float = 8.0`    |             |
| `max_power_1` | `float = 70.0`   |             |
| _Returns_     | `None`           |             |

### `end_job()`

```python
end_job() -> None
```

Finalize the job.

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| _Returns_ | `None` |             |

### `frequency()`

```python
frequency(frequency: float) -> None
```

Save the layer frequency for the next flush.

| Parameter   | Type    | Description |
| ----------- | ------- | ----------- |
| `frequency` | `float` |             |
| _Returns_   | `None`  |             |

### `move_speed()`

```python
move_speed(speed: float) -> None
```

Record a move speed (comment-only).

| Parameter | Type    | Description |
| --------- | ------- | ----------- |
| `speed`   | `float` |             |
| _Returns_ | `None`  |             |

### `move_x_to()`

```python
move_x_to(x: float) -> None
```

Move to an absolute X position.

| Parameter | Type    | Description |
| --------- | ------- | ----------- |
| `x`       | `float` |             |
| _Returns_ | `None`  |             |

### `move_xy_to()`

```python
move_xy_to(x: float, y: float) -> None
```

Move to an absolute XY position.

| Parameter | Type    | Description |
| --------- | ------- | ----------- |
| `x`       | `float` |             |
| `y`       | `float` |             |
| _Returns_ | `None`  |             |

### `move_y_to()`

```python
move_y_to(y: float) -> None
```

Move to an absolute Y position.

| Parameter | Type    | Description |
| --------- | ------- | ----------- |
| `y`       | `float` |             |
| _Returns_ | `None`  |             |

### `new_gluescript()`

```python
new_gluescript() -> None
```

Reset all per-job state.

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| _Returns_ | `None` |             |

### `power()`

```python
power(percent: Optional[float]) -> None
```

Set immediate power for an IMAGE/DEPTHMAP layer.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `percent` | `Optional[float]` |             |
| _Returns_ | `None`            |             |

### `power_range()`

```python
power_range(min_power: Optional[float], max_power: Optional[float]) -> None
```

Save a power ramp range for the next cut.

| Parameter   | Type              | Description |
| ----------- | ----------------- | ----------- |
| `min_power` | `Optional[float]` |             |
| `max_power` | `Optional[float]` |             |
| _Returns_   | `None`            |             |

### `pwm()`

```python
pwm(duration: float) -> None
```

Record the pulse width (comment-only).

| Parameter  | Type    | Description |
| ---------- | ------- | ----------- |
| `duration` | `float` |             |
| _Returns_  | `None`  |             |

### `select_laser()`

```python
select_laser(laser: int) -> None
```

Select a laser head.

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| `laser`   | `int`  |             |
| _Returns_ | `None` |             |

### `set_max_cut_speed()`

```python
set_max_cut_speed(speed: float) -> None
```

Set the maximum cut speed used by power scaling.

| Parameter | Type    | Description |
| --------- | ------- | ----------- |
| `speed`   | `float` |             |
| _Returns_ | `None`  |             |

### `set_mode()`

```python
set_mode(mode: str) -> None
```

Switch the layer mode mid-stream.

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| `mode`    | `str`  |             |
| _Returns_ | `None` |             |

### `set_overscan()`

```python
set_overscan(overscan: str) -> None
```

Set the overscan for the current layer.

| Parameter  | Type   | Description |
| ---------- | ------ | ----------- |
| `overscan` | `str`  |             |
| _Returns_  | `None` |             |

### `set_power_floor()`

```python
set_power_floor(floor: float) -> None
```

Set the laser's minimum firing power.

| Parameter | Type    | Description |
| --------- | ------- | ----------- |
| `floor`   | `float` |             |
| _Returns_ | `None`  |             |

### `set_power_scaling_enabled()`

```python
set_power_scaling_enabled(enabled: bool) -> None
```

Enable or disable effective-min power scaling.

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| `enabled` | `bool` |             |
| _Returns_ | `None` |             |

### `stage_gluescript()`

```python
stage_gluescript(
    gluescript: Optional[Sequence[str]] = None,
    require_complete: bool = True,
) -> str
```

Compile the job, or replay a transcript and compile it.

| Parameter          | Type                             | Description |
| ------------------ | -------------------------------- | ----------- |
| `gluescript`       | `Optional[Sequence[str]] = None` |             |
| `require_complete` | `bool = True`                    |             |
| _Returns_          | `str`                            |             |

## RuidaDriver(driver.Driver)

Inherits from `driver.Driver`; the inherited members are documented on the base class.

A direct connection to a Ruida laser controller.

All async methods return asyncio futures; a typed `DriverEvent` is delivered to
`event_callback(event)` on the event loop thread. `run()` and `run_raw()` take rpascript text; the
machine-code encoder produces it from Ops.

### `machine_status`

```python
machine_status: dict
```

The last decoded controller status.

### `protect_enabled`

```python
protect_enabled: bool
```

Whether protect mode blocks `SET_SETTING`.

### `execute()`

```python
execute(line: str) -> Awaitable[None]
```

Send one raw rpascript line.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `line`    | `str`             |             |
| _Returns_ | `Awaitable[None]` |             |

### `focus_z()`

```python
focus_z() -> Awaitable[None]
```

Run the controller's Z auto-focus routine.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `get_setting()`

```python
get_setting(address: int) -> Awaitable[list]
```

Read one controller memory address; returns `(address, data)` pairs.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `address` | `int`             |             |
| _Returns_ | `Awaitable[list]` |             |

### `home_u()`

```python
home_u() -> Awaitable[None]
```

Home the U axis.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `home_xy()`

```python
home_xy() -> Awaitable[None]
```

Move the XY axes to the current origin.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `home_z()`

```python
home_z() -> Awaitable[None]
```

Home the Z axis through the controller's homing routine.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `jog_axis_rel()`

```python
jog_axis_rel(axis: str, delta: float) -> Awaitable[None]
```

Jog one axis relatively in millimetres.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `axis`    | `str`             |             |
| `delta`   | `float`           |             |
| _Returns_ | `Awaitable[None]` |             |

### `jog_u_to()`

```python
jog_u_to(u: float) -> Awaitable[None]
```

Jog to an absolute U position in millimetres.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `u`       | `float`           |             |
| _Returns_ | `Awaitable[None]` |             |

### `jog_x_to()`

```python
jog_x_to(x: float) -> Awaitable[None]
```

Jog to an absolute X position in millimetres.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `x`       | `float`           |             |
| _Returns_ | `Awaitable[None]` |             |

### `jog_xy_rel()`

```python
jog_xy_rel(
    x: Optional[float] = None,
    y: Optional[float] = None,
) -> Awaitable[None]
```

Jog XY relatively in millimetres.

| Parameter | Type                     | Description |
| --------- | ------------------------ | ----------- |
| `x`       | `Optional[float] = None` |             |
| `y`       | `Optional[float] = None` |             |
| _Returns_ | `Awaitable[None]`        |             |

### `jog_xy_to()`

```python
jog_xy_to(x: float, y: float) -> Awaitable[None]
```

Jog to an absolute XY position in millimetres.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `x`       | `float`           |             |
| `y`       | `float`           |             |
| _Returns_ | `Awaitable[None]` |             |

### `jog_y_to()`

```python
jog_y_to(y: float) -> Awaitable[None]
```

Jog to an absolute Y position in millimetres.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `y`       | `float`           |             |
| _Returns_ | `Awaitable[None]` |             |

### `jog_z_to()`

```python
jog_z_to(z: float) -> Awaitable[None]
```

Jog to an absolute Z position in millimetres.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `z`       | `float`           |             |
| _Returns_ | `Awaitable[None]` |             |

### `reset()`

```python
reset() -> Awaitable[None]
```

Stop the job and home XY, like the panel's reset key.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| _Returns_ | `Awaitable[None]` |             |

### `set_jog_speeds()`

```python
set_jog_speeds(xy: float, z: float, u: float) -> None
```

Set the jog speeds in mm/s.

| Parameter | Type    | Description |
| --------- | ------- | ----------- |
| `xy`      | `float` |             |
| `z`       | `float` |             |
| `u`       | `float` |             |
| _Returns_ | `None`  |             |

### `set_protect()`

```python
set_protect(enabled: bool) -> None
```

Block or allow `SET_SETTING` commands.

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| `enabled` | `bool` |             |
| _Returns_ | `None` |             |

### `set_setting()`

```python
set_setting(address: int, a: int, b: int) -> Awaitable[None]
```

Write one controller memory address. Blocked while protect mode is on.

| Parameter | Type              | Description |
| --------- | ----------------- | ----------- |
| `address` | `int`             |             |
| `a`       | `int`             |             |
| `b`       | `int`             |             |
| _Returns_ | `Awaitable[None]` |             |

## Functions

### `encode_script()`

```python
encode_script(lines: Sequence[str]) -> list[int]
```

Encode rpascript lines into the controller's raw byte stream, including the file checksum.

| Parameter | Type            | Description |
| --------- | --------------- | ----------- |
| `lines`   | `Sequence[str]` |             |
| _Returns_ | `list[int]`     |             |
