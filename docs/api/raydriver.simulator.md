---
title: raydriver.simulator
sidebar_label: raydriver.simulator
---

A simulated machine with no physical device behind it.

Runs jobs with a configurable per-op delay, accepts moves and settings, mirrors work-coordinate
offsets and returns a fixed probe point. Intended for UI previews and tests.

## NoDeviceDriver(driver.Driver)

Inherits from `driver.Driver`; the inherited members are documented on the base class.

A simulated machine: no connection, no transport.
