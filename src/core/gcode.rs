//! Protocol-neutral G-code text helpers.
//!
//! Several sessions need the same line cleanup and WCS-slot
//! conversion. Keeping them here avoids protocol modules importing
//! each other for behavior that is not protocol-specific.

use std::sync::LazyLock;

use regex::Regex;

static GCODE_COMMENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\([^)]*\)").unwrap());

/// Strip G-code comments from a line: everything after `;` and
/// content between parentheses.
pub fn strip_gcode_comments(line: &str) -> String {
    let mut line = GCODE_COMMENT_RE.replace_all(line, "").to_string();
    if let Some(idx) = line.find(';') {
        line.truncate(idx);
    }
    line.trim().to_string()
}

/// Convert a G-code WCS name (e.g. `G54`) to its P-number.
pub fn gcode_to_p_number(wcs_slot: &str) -> Option<i64> {
    let rest = wcs_slot.strip_prefix('G')?;
    let num: i64 = rest.parse().ok()?;
    let p_num = num - 53;
    if (1..=6).contains(&p_num) {
        Some(p_num)
    } else {
        None
    }
}
