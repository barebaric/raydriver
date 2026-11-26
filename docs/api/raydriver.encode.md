---
title: raydriver.encode
sidebar_label: raydriver.encode
---

## EpilogSpec

Parameters for the Epilog print-job encoder.

`context_json` is a JSON-serialised `EpilogContext`: model family, job title, resolution, frequency,
autofocus, bed size, speed reference, origin layout and the rotary flag.

### `context_json`

```python
context_json: str
```

## GcodeDialectSpec

Typed Python wrapper around the Rust `GcodeDialectSpec`.

Constructed in Python with keyword arguments; the inner Rust struct is passed directly to the
encoder without a serde round-trip.

## GcodeSpec

Parameters for the G-code encoder.

### `context_json`

```python
context_json: str
```

### `dialect`

```python
dialect: GcodeDialectSpec
```

## RuidaSpec

Typed Python wrapper around the Rust `RuidaSpec`.

### `context_json`

```python
context_json: str
```
