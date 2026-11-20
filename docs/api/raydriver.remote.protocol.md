---
title: raydriver.remote.protocol
sidebar_label: raydriver.remote.protocol
---

## Constants

### AUTH

`str`

### AUTH_FAILED

`str`

### BAD_REQUEST

`str`

### BAD_VERSION

`str`

### CLIENT_LIST

`str`

### DEFAULT_KEEPALIVE_INTERVAL

`float`

### DEFAULT_KEEPALIVE_MISSES

`int`

### DEVICE_LIST

`str`

### EVENT

`str`

### FRAME_TOO_LARGE

`str`

### INTERNAL

`str`

### JOB_RUNNING

`str`

### KIND_JOB_ERROR

`str`

### KIND_LEASE

`str`

### KIND_PROGRESS

`str`

### LEASE_HELD

`str`

### LEASE_TAKEN

`str`

### LENGTH_PREFIX

`LengthPrefix`

### MAX_FRAME_BYTES

`int`

### PING

`str`

### PONG

`str`

### PROTOCOLS

`list[str]`

### PROTOCOL_LIST

`str`

### PROTOCOL_VERSION

`int`

### ProtocolError

`type`

### REQUEST_TYPES

`list[str]`

### SESSION_ATTACH

`str`

### SESSION_CLOSE

`str`

### SESSION_COMMAND

`str`

### SESSION_DETACH

`str`

### SESSION_LIST

`str`

### SESSION_METHODS

`list[str]`

### SESSION_OPEN

`str`

### SESSION_PROPERTIES

`list[str]`

### SESSION_PROPERTY

`str`

### UNKNOWN_DEVICE

`str`

### UNKNOWN_METHOD

`str`

### UNKNOWN_PROPERTY

`str`

### UNKNOWN_SESSION

`str`

### UNKNOWN_TYPE

`str`

### WELCOME

`str`

## LengthPrefix

### `size`

```python
size: int
```

### `pack()`

```python
pack(value: int) -> bytes
```

| Parameter | Type    | Description |
| --------- | ------- | ----------- |
| `value`   | `int`   |             |
| _Returns_ | `bytes` |             |

## Functions

### `decode_event()`

```python
decode_event(payload: Any) -> Optional[Any]
```

| Parameter | Type            | Description |
| --------- | --------------- | ----------- |
| `payload` | `Any`           |             |
| _Returns_ | `Optional[Any]` |             |

### `decode_frame()`

```python
decode_frame(payload: Sequence[int], max_bytes: Optional[int] = None) -> Any
```

| Parameter   | Type                   | Description |
| ----------- | ---------------------- | ----------- |
| `payload`   | `Sequence[int]`        |             |
| `max_bytes` | `Optional[int] = None` |             |
| _Returns_   | `Any`                  |             |

### `decode_state()`

```python
decode_state(payload: Any) -> types.DeviceState
```

| Parameter | Type                | Description |
| --------- | ------------------- | ----------- |
| `payload` | `Any`               |             |
| _Returns_ | `types.DeviceState` |             |

### `encode_event()`

```python
encode_event(event: core.SessionEvent) -> Any
```

| Parameter | Type                | Description |
| --------- | ------------------- | ----------- |
| `event`   | `core.SessionEvent` |             |
| _Returns_ | `Any`               |             |

### `encode_frame()`

```python
encode_frame(message: Any, max_bytes: Optional[int] = None) -> bytes
```

| Parameter   | Type                   | Description |
| ----------- | ---------------------- | ----------- |
| `message`   | `Any`                  |             |
| `max_bytes` | `Optional[int] = None` |             |
| _Returns_   | `bytes`                |             |

### `encode_state()`

```python
encode_state(state: types.DeviceState) -> Any
```

| Parameter | Type                | Description |
| --------- | ------------------- | ----------- |
| `state`   | `types.DeviceState` |             |
| _Returns_ | `Any`               |             |

### `error_response()`

```python
error_response(msg_id: int, code: str, message: str) -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `msg_id`  | `int` |             |
| `code`    | `str` |             |
| `message` | `str` |             |
| _Returns_ | `Any` |             |

### `event_frame()`

```python
event_frame(session_id: str, seq: int, event: Any) -> Any
```

| Parameter    | Type  | Description |
| ------------ | ----- | ----------- |
| `session_id` | `str` |             |
| `seq`        | `int` |             |
| `event`      | `Any` |             |
| _Returns_    | `Any` |             |

### `read_frame()`

```python
read_frame(reader: Any) -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `reader`  | `Any` |             |
| _Returns_ | `Any` |             |

### `request()`

```python
request(msg_id: int, type_: str, payload: Optional[Any] = None) -> Any
```

| Parameter | Type                   | Description |
| --------- | ---------------------- | ----------- |
| `msg_id`  | `int`                  |             |
| `type_`   | `str`                  |             |
| `payload` | `Optional[Any] = None` |             |
| _Returns_ | `Any`                  |             |

### `response()`

```python
response(msg_id: int, payload: Optional[Any] = None) -> Any
```

| Parameter | Type                   | Description |
| --------- | ---------------------- | ----------- |
| `msg_id`  | `int`                  |             |
| `payload` | `Optional[Any] = None` |             |
| _Returns_ | `Any`                  |             |

### `welcome_payload()`

```python
welcome_payload(raydriver_version: str, auth_required: bool) -> Any
```

| Parameter           | Type   | Description |
| ------------------- | ------ | ----------- |
| `raydriver_version` | `str`  |             |
| `auth_required`     | `bool` |             |
| _Returns_           | `Any`  |             |

### `write_frame()`

```python
write_frame(writer: Any, message: Any) -> Any
```

| Parameter | Type  | Description |
| --------- | ----- | ----------- |
| `writer`  | `Any` |             |
| `message` | `Any` |             |
| _Returns_ | `Any` |             |
