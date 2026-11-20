---
title: raydriver.protocols
sidebar_label: raydriver.protocols
---

## Constants

### PROTOCOLS

`dict[str, ProtocolInfo]`

## ProtocolInfo

Metadata describing one session protocol.

`capabilities` is derived from the method/property surface so the two never drift apart.
`dialect_family` is a hint clients can map to their own command-dialect definitions; it is not a
driver selection.

### `capabilities`

```python
capabilities: dict[str, bool]
```

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

### `label`

```python
label: str
```

### `methods`

```python
methods: list[str]
```

### `name`

```python
name: str
```

### `properties`

```python
properties: list[str]
```

### `to_dict()`

```python
to_dict() -> dict
```

The wire form served by `protocol/list`.

| Parameter | Type   | Description |
| --------- | ------ | ----------- |
| _Returns_ | `dict` |             |

## Functions

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
protocols() -> dict[str, ProtocolInfo]
```

Every protocol's metadata, keyed by protocol name.

| Parameter | Type                      | Description |
| --------- | ------------------------- | ----------- |
| _Returns_ | `dict[str, ProtocolInfo]` |             |

### `protocols_payload()`

```python
protocols_payload() -> dict[str, Any]
```

The `protocol/list` payload: every protocol's metadata.

| Parameter | Type             | Description |
| --------- | ---------------- | ----------- |
| _Returns_ | `dict[str, Any]` |             |
