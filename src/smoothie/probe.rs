//! Device census for Smoothieware controllers.
//!
//! Turns the raw ``version`` and ``config-get`` responses into the
//! structured census dict: firmware identity and the motion limits
//! applications need. Pure functions over parser output — no I/O —
//! so local sessions and remote daemon proxies are probed
//! identically.

use serde_json::{json, Value};

/// Commands the census queries, in order.
pub const CENSUS_COMMANDS: [&str; 6] = [
    "version",
    "config-get alpha_max",
    "config-get beta_max",
    "config-get alpha_max_rate",
    "config-get beta_max_rate",
    "config-get acceleration",
];

/// The first non-empty reply line, if any.
fn first_value(lines: &[String]) -> Option<String> {
    lines
        .iter()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
        .map(|line| line.to_string())
}

/// Drops ``ok`` acknowledgements and ``<...>`` status reports:
/// ``config-get`` replies arrive with a trailing ``ok``, and an
/// idle status poll can interleave; neither carries census data.
fn filter_reply(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed != "ok"
                && !(trimmed.starts_with('<') && trimmed.ends_with('>'))
        })
        .cloned()
        .collect()
}

/// Parses a ``config-get`` numeric reply into a float. Smoothieware
/// replies with the bare value (e.g. ``200.000``); non-numeric
/// replies yield ``None``.
fn parse_float(lines: &[String]) -> Option<f64> {
    first_value(&filter_reply(lines)).and_then(|raw| raw.parse::<f64>().ok())
}

/// Builds the census dict from the raw Smoothieware census-command
/// responses. Pure data transformation with no I/O; the caller is
/// responsible for communicating with the device and passing the
/// collected response lines.
pub fn describe(
    version_lines: &[String],
    alpha_max_lines: &[String],
    beta_max_lines: &[String],
    alpha_rate_lines: &[String],
    beta_rate_lines: &[String],
    acceleration_lines: &[String],
) -> Value {
    let alpha_max = parse_float(alpha_max_lines);
    let beta_max = parse_float(beta_max_lines);
    let alpha_rate = parse_float(alpha_rate_lines);
    let beta_rate = parse_float(beta_rate_lines);
    let acceleration = parse_float(acceleration_lines);

    let max_travel = match (alpha_max, beta_max) {
        (Some(alpha_max), Some(beta_max))
            if alpha_max > 0.0 && beta_max > 0.0 =>
        {
            json!([alpha_max, beta_max])
        }
        _ => Value::Null,
    };

    let max_speed = match (alpha_rate, beta_rate) {
        // config-get rates are in mm/s; machine configs are mm/min.
        (Some(alpha_rate), Some(beta_rate)) => {
            json!((alpha_rate.min(beta_rate) * 60.0) as i64)
        }
        _ => Value::Null,
    };

    json!({
        "firmware_version": first_value(&filter_reply(version_lines)),
        "alpha_max": alpha_max,
        "beta_max": beta_max,
        "alpha_rate": alpha_rate,
        "beta_rate": beta_rate,
        "acceleration": acceleration,
        "max_travel": max_travel,
        "max_speed": max_speed,
        "acceleration_steps": acceleration.map(|value| value as i64),
    })
}
