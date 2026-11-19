---
title: raydriver
sidebar_label: raydriver
---

## ProtocolInfo

One protocol's metadata: label, config shape, method surface and derived capabilities.

### `capabilities`

```python
capabilities: Any
```

What applications can do with the protocol, derived from the method/property surface so the two
never drift apart.

### `config_keys`

```python
config_keys: list[str]
```

### `connection`

```python
connection: str
```

### `dialect_family`

```python
dialect_family: str
```

A hint clients can map to their own command-dialect definitions; it is not a driver selection.

### `label`

```python
label: str
```

### `methods`

```python
methods: list[str]
```

The sorted method surface of the session.

### `name`

```python
name: str
```

### `properties`

```python
properties: list[str]
```

The sorted property surface of the session.

### `to_dict()`

```python
to_dict() -> Any
```

The wire form served by `protocol/list`.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

## Functions

### `init_logging()`

```python
init_logging(filter: Optional[str] = None, path: Optional[str] = None) -> None
```

Initialize the Rust-side logger (`log` crate) so session diagnostics reach stderr or, when *path* is
given, a dedicated log file (opened in append mode). The filter defaults to `warn` and can be
overridden with an `env_logger` filter string argument or via `RUST_LOG` (which always wins).
Calling again re-targets output and replaces the filter.

| Parameter | Type                   | Description |
| --------- | ---------------------- | ----------- |
| `filter`  | `Optional[str] = None` |             |
| `path`    | `Optional[str] = None` |             |
| _Returns_ | `None`                 |             |

### `protocol_info()`

```python
protocol_info(name: str) -> ProtocolInfo
```

Look up a protocol's metadata, raising `KeyError` if unknown.

| Parameter | Type           | Description |
| --------- | -------------- | ----------- |
| `name`    | `str`          |             |
| _Returns_ | `ProtocolInfo` |             |

### `protocols()`

```python
protocols() -> Any
```

The registry as a name → metadata mapping.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |

### `protocols_payload()`

```python
protocols_payload() -> Any
```

The `protocol/list` payload: every protocol's metadata.

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| _Returns_ | `Any` |             |
