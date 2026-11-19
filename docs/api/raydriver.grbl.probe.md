---
title: raydriver.grbl.probe
sidebar_label: raydriver.grbl.probe
---

## Functions

### `describe()`

```python
describe(build_info: Sequence[str], settings_lines: Sequence[str]) -> Any
```

Builds the census dict from the raw Grbl `$I` and `$$` response lines.

Pure data transformation with no I/O; the caller is responsible for communicating with the device
and passing the collected response lines.

| Parameter        | Type            | Description |
| ---------------- | --------------- | ----------- |
| `build_info`     | `Sequence[str]` |             |
| `settings_lines` | `Sequence[str]` |             |
| _Returns_        | `Any`           |             |
