---
title: raydriver.core.discovery.mdns_channel
sidebar_label: raydriver.core.discovery.mdns_channel
---

## Constants

### NETWORK_SCAN_TIMEOUT

`float`

### scan_mdns_services

`Any`

## Functions

### `collect_mdns_recognizers()`

```python
collect_mdns_recognizers(driver_classes: Sequence[Any]) -> list
```

Pairs every driver whose `DISCOVERY` spec declares a valid mDNS recognizer with it. Declared service
types and fingerprint service types are normalized to their canonical `_name._transport` form.

| Parameter        | Type            | Description |
| ---------------- | --------------- | ----------- |
| `driver_classes` | `Sequence[Any]` |             |
| _Returns_        | `list`          |             |

### `find_network_devices()`

```python
find_network_devices(driver_classes: Sequence[Any]) -> Any
```

Browses the network for every declared service type plus the generic types needed for
fingerprinting, and returns one device per confirmed match.

Like every channel, this never raises: failures are logged and bounded by a timeout.

| Parameter        | Type            | Description |
| ---------------- | --------------- | ----------- |
| `driver_classes` | `Sequence[Any]` |             |
| _Returns_        | `Any`           |             |
