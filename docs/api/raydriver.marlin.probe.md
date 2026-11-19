---
title: raydriver.marlin.probe
sidebar_label: raydriver.marlin.probe
---

## Functions

### `describe()`

```python
describe(
    m115_lines: Sequence[str],
    m211_lines: Sequence[str],
    m503_lines: Sequence[str],
    m149_lines: Sequence[str],
    boot_lines: Optional[Sequence[str]] = None,
) -> Any
```

Builds the census dict from the raw Marlin census-command responses.

Pure data transformation with no I/O; the caller is responsible for communicating with the device
and passing the collected response lines. `boot_lines` are startup messages observed before the
handshake; a version found there is more specific than the `FIRMWARE_NAME` token M115 reports.

| Parameter    | Type                             | Description |
| ------------ | -------------------------------- | ----------- |
| `m115_lines` | `Sequence[str]`                  |             |
| `m211_lines` | `Sequence[str]`                  |             |
| `m503_lines` | `Sequence[str]`                  |             |
| `m149_lines` | `Sequence[str]`                  |             |
| `boot_lines` | `Optional[Sequence[str]] = None` |             |
| _Returns_    | `Any`                            |             |
