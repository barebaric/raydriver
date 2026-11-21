//! Device census for OctoPrint servers.
//!
//! Interprets the parsed ``/api/version`` and ``/api/printer``
//! responses into the structured census dict: server identity and
//! the work area the server reports. Pure functions over API
//! payloads — no I/O — so local sessions and remote daemon proxies
//! are probed identically.

use serde_json::{json, Value};

/// A positive number from API JSON, else ``None`` (booleans and
/// non-numeric values are rejected).
fn positive_length(value: &Value) -> Option<f64> {
    if value.is_boolean() {
        return None;
    }
    let length = value.as_f64()?;
    (length > 0.0).then_some(length)
}

/// Builds the census dict from the parsed OctoPrint API payloads.
///
/// Pure data transformation with no I/O; the caller is responsible
/// for the HTTP requests. *version_info* is the ``GET
/// /api/version`` response, *printer_info* the ``GET
/// /api/printer?exclude=temperature,sd`` response (either may be
/// null when the request failed).
pub fn describe(version_info: &Value, printer_info: &Value) -> Value {
    let server_version = version_info
        .get("version")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(Value::from);

    let mut name = json!("OctoPrint");
    let mut max_travel = Value::Null;
    if let Some(printer) = printer_info.as_object() {
        // Only the static machine name feeds the profile name; baking
        // transient state text ("Heating bed...") into it would make
        // the persisted name drift with printer state.
        if let Some(name_value) = printer
            .get("printer")
            .and_then(|printer| printer.get("name"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            name = json!(name_value);
        }
        let lengths = printer_info.get("dimensions");
        let x = lengths
            .and_then(|dimensions| positive_length(&dimensions["x_length"]));
        let y = lengths
            .and_then(|dimensions| positive_length(&dimensions["y_length"]));
        if let (Some(x), Some(y)) = (x, y) {
            max_travel = json!([x, y]);
        }
    }

    json!({
        "server_version": server_version,
        "name": name,
        "max_travel": max_travel,
    })
}
