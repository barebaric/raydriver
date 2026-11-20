---
title: raydriver.remote.daemon
sidebar_label: raydriver.remote.daemon
---

## Constants

### DEFAULT_LEASE_GRACE

`float`

### DEFAULT_PORT

`int`

### DEFAULT_RING_SIZE

`int`

### SEND_QUEUE_LIMIT

`int`

## ClientQueue

### `get()`

```python
get() -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `put()`

```python
put(frame: Any) -> None
```

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| `frame`   | `Any`  |             |
| _Returns_ | `None` |             |

## Daemon

### `config`

```python
config: DaemonConfig
```

### `start()`

```python
start() -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `stop()`

```python
stop() -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

## DaemonConfig

### `cameras`

```python
cameras: Any
```

### `host`

```python
host: str
```

### `keepalive_interval`

```python
keepalive_interval: float
```

### `keepalive_misses`

```python
keepalive_misses: int
```

### `lease_grace`

```python
lease_grace: float
```

### `mdns`

```python
mdns: bool
```

### `port`

```python
port: int
```

### `ring_size`

```python
ring_size: int
```

### `sessions`

```python
sessions: Any
```

### `tls_cert`

```python
tls_cert: Optional[str]
```

### `tls_key`

```python
tls_key: Optional[str]
```

### `token`

```python
token: Optional[str]
```
