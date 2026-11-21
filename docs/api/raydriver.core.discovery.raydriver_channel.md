---
title: raydriver.core.discovery.raydriver_channel
sidebar_label: raydriver.core.discovery.raydriver_channel
---

## Constants

### REMOTE_DRIVER

`str`

### SERVICE_TYPE

`str`

### scan_mdns_services

`Callable[..., Awaitable[list[mdns_scan.MDNSService]]]`

## Functions

### `find_raydriver_devices()`

```python
find_raydriver_devices() -> Awaitable[list[discovery.DiscoveredDevice]]
```

Browse for raydriver daemons and list their sessions.

Like every channel, this never raises: failures are logged and bounded by timeouts.

| Parameter | Type                                          | Description |
| --------- | --------------------------------------------- | ----------- |
| _Returns_ | `Awaitable[list[discovery.DiscoveredDevice]]` |             |
