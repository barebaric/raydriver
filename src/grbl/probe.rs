//! Device census for GRBL controllers.
//!
//! Interprets the raw ``$I`` and ``$$`` responses into the
//! structured census dict: firmware identity, compile flags, flavor
//! classification (stock Grbl / grblHAL / FluidNC), the parsed
//! settings inventory, and the semantic values applications need
//! (work travel, motion limits, laser mode, unit reporting). Pure
//! functions over parser output — no I/O — so local sessions and
//! remote daemon proxies are probed identically.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Value};

use crate::grbl::parser::{
    extract_device_name, parse_grbl_settings, parse_opt_flags, parse_opt_info,
    parse_version,
};

/// Regexes matching grblHAL / FluidNC identifiers in a ``[VER:]``
/// line.
static HAL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("(?i)grblhal").unwrap());
static FLUIDNC_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("(?i)fluidnc").unwrap());

/// grblHAL exposes per-axis settings under the ``$4xx`` group
/// (e.g. ``$400`` for axis steps/mm). Stock Grbl uses ``$0``–
/// ``$255``.
static HAL_SETTING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$4\d{2}").unwrap());

/// The firmware family a device identifies itself as.
fn detect_flavor(
    build_info: &[String],
    settings_lines: &[String],
) -> &'static str {
    for line in build_info {
        if HAL_RE.is_match(line) {
            return "grblhal";
        }
        if FLUIDNC_RE.is_match(line) {
            return "fluidnc";
        }
    }
    if settings_lines
        .iter()
        .any(|line| HAL_SETTING_RE.is_match(line))
    {
        return "grblhal";
    }
    if parse_version(build_info).is_some() {
        return "stock";
    }
    "unknown"
}

/// Builds the census dict from the raw Grbl ``$I`` and ``$$``
/// response lines.
///
/// Pure data transformation with no I/O; the caller is responsible
/// for communicating with the device and passing the collected
/// response lines.
pub fn describe(build_info: &[String], settings_lines: &[String]) -> Value {
    let mut rx_buffer_size: Option<i64> = None;
    let mut compile_flags = String::new();
    for line in build_info {
        if let Some(rx) = parse_opt_info(line) {
            rx_buffer_size = Some(rx);
        }
        if let Some(flags) = parse_opt_flags(line) {
            compile_flags = flags;
        }
    }

    let settings: HashMap<String, f64> = parse_grbl_settings(settings_lines);
    let setting = |key: &str| settings.get(key).copied();

    let max_speed = match (setting("110"), setting("111")) {
        (Some(x), Some(y)) => json!((x.min(y)) as i64),
        _ => Value::Null,
    };
    let acceleration = match (setting("120"), setting("121")) {
        (Some(x), Some(y)) => json!((x.min(y)) as i64),
        _ => Value::Null,
    };
    let max_travel = match (setting("130"), setting("131")) {
        (Some(x), Some(y)) => json!([x, y]),
        _ => Value::Null,
    };

    json!({
        "name": extract_device_name(build_info),
        "firmware_version": parse_version(build_info),
        "rx_buffer_size": rx_buffer_size,
        "compile_flags": compile_flags,
        "flavor": detect_flavor(build_info, settings_lines),
        "settings": settings,
        "max_travel": max_travel,
        "max_speed": max_speed,
        "acceleration": acceleration,
        "max_spindle": setting("30").map(|value| value as i64),
        "laser_mode": setting("32").map(|value| value != 0.0),
        "homing_enabled": setting("22").map(|value| value != 0.0),
        "report_inches": setting("13").map(|value| value != 0.0),
        "arc_tolerance": setting("12"),
        "single_axis_homing": compile_flags.contains('H'),
    })
}
