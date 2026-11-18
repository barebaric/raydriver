---
title: raydriver
sidebar_label: raydriver
---

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
