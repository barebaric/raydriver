---
title: raydriver.remote.inspect
sidebar_label: raydriver.remote.inspect
---

## Functions

### `daemon_rpc()`

```python
daemon_rpc(
    host: str,
    port: int,
    token: Optional[str],
    tls_ca: Optional[str],
    requests: Any,
) -> Any
```

| Parameter  | Type            | Description |
| ---------- | --------------- | ----------- |
| `host`     | `str`           |             |
| `port`     | `int`           |             |
| `token`    | `Optional[str]` |             |
| `tls_ca`   | `Optional[str]` |             |
| `requests` | `Any`           |             |
| _Returns_  | `Any`           |             |

### `snapshot()`

```python
snapshot(
    host: str,
    port: int,
    token: Optional[str] = None,
    tls_ca: Optional[str] = None,
) -> Any
```

| Parameter | Type                   | Description |
| --------- | ---------------------- | ----------- |
| `host`    | `str`                  |             |
| `port`    | `int`                  |             |
| `token`   | `Optional[str] = None` |             |
| `tls_ca`  | `Optional[str] = None` |             |
| _Returns_ | `Any`                  |             |

### `welcome_probe()`

```python
welcome_probe(host: str, port: int) -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `host`    | `str` |             |
| `port`    | `int` |             |
| _Returns_ | `Any` |             |
