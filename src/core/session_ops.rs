//! Command construction shared by the protocol sessions.
//!
//! Formatting device operations from the dialect is protocol-neutral;
//! the sessions differ only in how they transmit the resulting lines.

use crate::core::dialect::{format_template, Arg, Dialect};
use crate::core::gcode::gcode_to_p_number;
use crate::core::session::SessionError;

/// Home commands for the given axes, optionally re-activating the
/// active WCS with the GRBL toggle workaround.
pub fn home_commands(
    dialect: &Dialect,
    axes: Option<&[String]>,
    active_wcs: Option<&str>,
) -> Result<Vec<String>, SessionError> {
    let mut commands = Vec::new();
    match axes {
        None => commands.push(dialect.home_all.clone()),
        Some(axes) => {
            for axis in axes {
                let cmd = format_template(
                    &dialect.home_axis,
                    &[("axis_letter", Arg::Str(axis.clone()))],
                )
                .map_err(SessionError::Other)?;
                commands.push(cmd);
            }
        }
    }
    if let Some(active_wcs) = active_wcs {
        // Some Grbl versions forget the G54 offset after homing;
        // toggling to another WCS and back re-activates it.
        let temp_wcs = if active_wcs == "G54" { "G55" } else { "G54" };
        commands.push("G4 P0.01".to_string());
        commands.push(temp_wcs.to_string());
        commands.push(active_wcs.to_string());
    }
    Ok(commands)
}

pub fn move_to_command(
    dialect: &Dialect,
    speed: f64,
    pos_x: f64,
    pos_y: f64,
) -> Result<String, SessionError> {
    format_template(
        &dialect.move_to,
        &[
            ("speed", Arg::Speed(speed)),
            ("x", Arg::Length(pos_x)),
            ("y", Arg::Length(pos_y)),
        ],
    )
    .map_err(SessionError::Other)
}

/// Build a jog command, or `None` when there are no deltas.
pub fn jog_command(
    dialect: &Dialect,
    speed: f64,
    deltas: &[(String, f64)],
) -> Result<Option<String>, SessionError> {
    let head = format_template(&dialect.jog, &[("speed", Arg::Speed(speed))])
        .map_err(SessionError::Other)?;
    if deltas.is_empty() {
        return Ok(None);
    }
    let mut parts = vec![head];
    for (axis_name, distance) in deltas {
        parts.push(format!(
            "{}{}",
            axis_name.to_uppercase(),
            crate::core::dialect::python_float_repr(*distance)
        ));
    }
    Ok(Some(parts.join(" ")))
}

pub fn select_tool_command(
    dialect: &Dialect,
    tool_number: i64,
) -> Result<String, SessionError> {
    format_template(
        &dialect.tool_change,
        &[("tool_number", Arg::Int(tool_number))],
    )
    .map_err(SessionError::Other)
}

/// Build a laser-on/off command. `power` of `None` or `0` turns the
/// output off.
pub fn power_command(
    dialect: &Dialect,
    power: Option<f64>,
    focus: bool,
) -> Result<String, SessionError> {
    let template = if focus {
        &dialect.focus_laser_on
    } else {
        &dialect.laser_on
    };
    match power {
        Some(p) if p > 0.0 => {
            format_template(template, &[("power", Arg::Length(p))])
                .map_err(SessionError::Other)
        }
        _ => Ok(dialect.laser_off.clone()),
    }
}

pub fn wcs_offset_command(
    dialect: &Dialect,
    wcs_slot: &str,
    x: f64,
    y: f64,
    z: Option<f64>,
) -> Result<String, SessionError> {
    let p_num = gcode_to_p_number(wcs_slot).ok_or_else(|| {
        SessionError::Other(format!("Invalid WCS slot: {wcs_slot}"))
    })?;
    Ok(dialect.format_wcs_offset(p_num, x, y, z))
}

pub fn probe_cycle_command(
    dialect: &Dialect,
    axis_letter: &str,
    max_travel: f64,
    feed_rate: f64,
) -> Result<String, SessionError> {
    format_template(
        &dialect.probe_cycle,
        &[
            ("axis_letter", Arg::Str(axis_letter.to_string())),
            ("max_travel", Arg::Length(max_travel)),
            ("feed_rate", Arg::Speed(feed_rate)),
        ],
    )
    .map_err(SessionError::Other)
}

/// Resolved safety-off commands, plus an optional emergency stop.
pub fn safety_shutdown_commands(
    dialect: &Dialect,
    emergency: bool,
) -> Vec<String> {
    let mut commands = dialect.safety_off_commands.clone();
    if emergency {
        if let Some(cmd) = dialect.emergency_stop.clone() {
            commands.push(cmd);
        }
    }
    commands
}
