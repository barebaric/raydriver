//! Device census for Marlin controllers.
//!
//! Interprets the raw ``M115``, ``M211``, ``M503`` and ``M149``
//! responses into the structured census dict: firmware identity,
//! software-endstop travel and motion limits. Pure functions over
//! parser output — no I/O — so local sessions and remote daemon
//! proxies are probed identically.

use serde_json::{json, Value};

use crate::core::types::UnitSystem;
use crate::marlin::parser::{
    detect_unit_system_from_m149, extract_marlin_device_name, is_ok,
    parse_m115_firmware_info, parse_m211_endstops, parse_m503_settings,
    parse_marlin_version,
};

/// Commands the census queries, in order.
pub const CENSUS_COMMANDS: [&str; 4] = ["M115", "M211", "M503", "M149"];

fn firmware_name(info: &[(String, String)]) -> Option<String> {
    info.iter()
        .find(|(key, _)| key == "firmware_name")
        .map(|(_, value)| value.clone())
}

fn machine_type(info: &[(String, String)]) -> Option<String> {
    info.iter()
        .find(|(key, _)| key == "machine_type")
        .map(|(_, value)| value.clone())
}

/// Builds the census dict from the raw Marlin census-command
/// responses.
///
/// Pure data transformation with no I/O; the caller is responsible
/// for communicating with the device and passing the collected
/// response lines. *boot_lines* are startup messages observed
/// before the handshake; a version found there is more specific
/// than the ``FIRMWARE_NAME`` token M115 reports.
pub fn describe(
    m115_lines: &[String],
    m211_lines: &[String],
    m503_lines: &[String],
    m149_lines: &[String],
    boot_lines: &[String],
) -> Value {
    let name = extract_marlin_device_name(m115_lines, boot_lines);

    let info = parse_m115_firmware_info(m115_lines);
    let mut firmware_version = firmware_name(&info);
    for line in boot_lines {
        if let Some(boot_version) = parse_marlin_version(line) {
            firmware_version = Some(boot_version);
            break;
        }
    }

    let max_travel = parse_m211_endstops(m211_lines)
        .filter(|(x_max, y_max)| *x_max > 0.0 && *y_max > 0.0)
        .map(|(x_max, y_max)| json!([x_max, y_max]));

    let m503 = parse_m503_settings(m503_lines);
    let setting = |key: &str| {
        m503.iter()
            .find(|(setting_key, _)| setting_key == key)
            .map(|(_, value)| *value)
    };
    let max_speed = match (setting("max_feedrate_x"), setting("max_feedrate_y"))
    {
        (Some(x), Some(y)) => json!((x.min(y) * 60.0) as i64),
        _ => Value::Null,
    };
    let acceleration = setting("acceleration").map(|value| value as i64);

    let units =
        detect_unit_system_from_m149(m149_lines).map(|unit| match unit {
            UnitSystem::Metric => "metric",
            UnitSystem::Imperial => "imperial",
        });

    json!({
        "name": name,
        "firmware_version": firmware_version,
        "machine_type": machine_type(&info),
        "max_travel": max_travel,
        "max_speed": max_speed,
        "acceleration": acceleration,
        "units": units,
    })
}

/// Drops ``ok`` acknowledgements: the parsers match semantic lines
/// only, and ``ok`` carries no census data.
/// Drops `ok` acknowledgements: the parsers match semantic
/// lines only, and `ok` carries no census data.
pub fn filter_ok_lines(lines: &[String]) -> Vec<String> {
    lines.iter().filter(|line| !is_ok(line)).cloned().collect()
}
