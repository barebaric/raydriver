---
title: raydriver.core.discovery.fingerprints
sidebar_label: raydriver.core.discovery.fingerprints
---

## TcpFingerprint

Confirms a generic TCP service by matching a response line.

The probe waits *banner_wait* seconds for an unsolicited banner, then (when *probe_command* is set)
sends it and waits for the remaining *timeout* seconds. It returns a truthy value when a line starts
with *match_prefix*, and `None` otherwise.

### `banner_wait`

```python
banner_wait: float
```

### `match_prefix`

```python
match_prefix: str
```

### `probe_command`

```python
probe_command: Optional[str]
```

### `timeout`

```python
timeout: float
```
