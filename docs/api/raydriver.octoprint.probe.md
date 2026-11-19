---
title: raydriver.octoprint.probe
sidebar_label: raydriver.octoprint.probe
---

## Functions

### `describe()`

```python
describe(
    version_info: Optional[Any] = None,
    printer_info: Optional[Any] = None,
) -> Any
```

Builds the census dict from the parsed OctoPrint API payloads.

Pure data transformation with no I/O; the caller is responsible for the HTTP requests.
*version_info* is the `GET /api/version` response, *printer_info* the `GET /api/printer` response
(either may be `None` when the request failed).

| Parameter      | Type                   | Description |
| -------------- | ---------------------- | ----------- |
| `version_info` | `Optional[Any] = None` |             |
| `printer_info` | `Optional[Any] = None` |             |
| _Returns_      | `Any`                  |             |
