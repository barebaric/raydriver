---
title: raydriver.smoothie.probe
sidebar_label: raydriver.smoothie.probe
---

## Functions

### `describe()`

```python
describe(
    version_lines: Sequence[str],
    alpha_max_lines: Sequence[str],
    beta_max_lines: Sequence[str],
    alpha_max_rate_lines: Sequence[str],
    beta_max_rate_lines: Sequence[str],
    acceleration_lines: Sequence[str],
) -> Any
```

Builds the census dict from raw Smoothieware census-command responses.

Pure data transformation with no I/O; the caller is responsible for communicating with the device
and passing the collected response lines. `ok` acknowledgements and `<...>` status reports are
dropped, and non-numeric `config-get` replies yield `None` fields.

| Parameter              | Type            | Description |
| ---------------------- | --------------- | ----------- |
| `version_lines`        | `Sequence[str]` |             |
| `alpha_max_lines`      | `Sequence[str]` |             |
| `beta_max_lines`       | `Sequence[str]` |             |
| `alpha_max_rate_lines` | `Sequence[str]` |             |
| `beta_max_rate_lines`  | `Sequence[str]` |             |
| `acceleration_lines`   | `Sequence[str]` |             |
| _Returns_              | `Any`           |             |
