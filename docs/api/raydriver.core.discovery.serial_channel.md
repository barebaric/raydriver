---
title: raydriver.core.discovery.serial_channel
sidebar_label: raydriver.core.discovery.serial_channel
---

## Constants

### CORPORATE_TOKENS

`set[str]`

### GENERIC_DEVICE_TOKENS

`set[str]`

### GENERIC_TOKENS

`set[str]`

### SERIAL_SCAN_TIMEOUT

`float`

### USB_CHIP_TOKENS

`set[str]`

### scan_serial_ports

`Any`

## Functions

### `build_identity()`

```python
build_identity(
    firmware: Optional[str],
    port_info: Optional[ports.SerialPortInfo],
    device_name: Optional[str] = None,
) -> discovery.DeviceIdentity
```

Assembles a `DeviceIdentity` from serial scan artifacts. A *device_name* extracted from the device's
own output (when the recognizer provides one) is the identity banner and contributes matching
tokens.

| Parameter     | Type                             | Description |
| ------------- | -------------------------------- | ----------- |
| `firmware`    | `Optional[str]`                  |             |
| `port_info`   | `Optional[ports.SerialPortInfo]` |             |
| `device_name` | `Optional[str] = None`           |             |
| _Returns_     | `discovery.DeviceIdentity`       |             |

### `collect_serial_recognizers()`

```python
collect_serial_recognizers(driver_classes: Sequence[Any]) -> list
```

Pairs every driver whose `DISCOVERY` spec declares a valid serial recognizer with it.

| Parameter        | Type            | Description |
| ---------------- | --------------- | ----------- |
| `driver_classes` | `Sequence[Any]` |             |
| _Returns_        | `list`          |             |

### `find_serial_devices()`

```python
find_serial_devices(
    driver_classes: Sequence[Any],
    ports: Optional[Iterable[str]] = None,
    baud_rates: Optional[Iterable[int]] = None,
    exclude_ports: Optional[Iterable[str]] = None,
) -> Any
```

Runs one serial scan and evaluates it against every declared serial recognizer. Like every channel,
this never raises: failures are logged, and the scan is bounded by *SERIAL_SCAN_TIMEOUT*, so a slow
or silent adapter can at most cost its own probe time.

| Parameter        | Type                             | Description |
| ---------------- | -------------------------------- | ----------- |
| `driver_classes` | `Sequence[Any]`                  |             |
| `ports`          | `Optional[Iterable[str]] = None` |             |
| `baud_rates`     | `Optional[Iterable[int]] = None` |             |
| `exclude_ports`  | `Optional[Iterable[str]] = None` |             |
| _Returns_        | `Any`                            |             |

### `normalize_tokens()`

```python
normalize_tokens(*texts: Any) -> frozenset[str]
```

Splits arbitrary identification strings (descriptions, by-id link names, manufacturer strings) into
lowercase word tokens suitable for containment checks.

| Parameter | Type             | Description |
| --------- | ---------------- | ----------- |
| `*texts`  | `Any`            |             |
| _Returns_ | `frozenset[str]` |             |
