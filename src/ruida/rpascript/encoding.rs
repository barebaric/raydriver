//! rpascript encoding.
//!
//! Counterpart of ``ruida_pa.rpascript.encoding``.

use std::fmt;

use crate::ruida::protocols::ruida::ruida_protocol::{
    CommandDef, Param, END_JOB_BYTES, END_JOB_OPCODE, FILE_PREFIX,
    KEYPAD_PREFIX, SETTING_PREFIX,
};
use crate::ruida::rpalib::ruida_transcoder::{
    encode_string8, from_int, from_uint, round_half_even, swap_red_blue,
};
use crate::ruida::rpascript::interpreter::{
    lookup, memory_address, parse_script, Command,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    UnknownMnemonic(String),
    UnknownMemory(String),
    BadParam { mnemonic: String, value: String },
    BadScript(String),
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownMnemonic(name) => {
                write!(f, "unknown rpascript command {name:?}")
            }
            Self::UnknownMemory(name) => {
                write!(f, "unknown memory address {name:?}")
            }
            Self::BadParam { mnemonic, value } => {
                write!(f, "invalid value {value:?} for {mnemonic}")
            }
            Self::BadScript(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for CodecError {}

impl CommandDef {
    /// The bytes that precede the parameters.
    fn header(&self) -> Vec<u8> {
        let mut header = vec![self.prefix];
        if let Some(mid) = self.mid {
            header.push(mid & 0x7F);
        }
        if let Some(inner) = self.inner {
            header.push(inner & 0x7F);
        }
        header
    }

    fn is_end_job(&self) -> bool {
        self.prefix == FILE_PREFIX && self.mid == Some(END_JOB_OPCODE)
    }

    fn counts_toward_checksum(&self) -> bool {
        self.prefix != KEYPAD_PREFIX
            && self.prefix != SETTING_PREFIX
            && !self.is_end_job()
    }
}

pub fn encode_command(command: &Command) -> Result<Vec<u8>, CodecError> {
    let def = lookup(&command.mnemonic)
        .ok_or_else(|| CodecError::UnknownMnemonic(command.mnemonic.clone()))?;
    let mut raw = def.header();
    for (param, token) in def.params.iter().zip(&command.params) {
        raw.extend(encode_param(*param, token).ok_or_else(|| {
            CodecError::BadParam {
                mnemonic: command.mnemonic.clone(),
                value: token.to_string(),
            }
        })?);
    }
    Ok(raw)
}

/// Encodes commands into the raw byte stream, filling every
/// ``END_JOB`` checksum with the sum of the bytes that precede it.
///
/// ``SET_SETTING`` (``0xDA``), keypress (``0xA7``) and ``END_JOB``
/// itself do not count; ``EOF`` does.
pub fn encode_commands(commands: &[Command]) -> Result<Vec<u8>, CodecError> {
    let mut stream = Vec::new();
    let mut checksum_offsets = Vec::new();
    let mut checksum: u64 = 0;
    for command in commands {
        let def = lookup(&command.mnemonic).ok_or_else(|| {
            CodecError::UnknownMnemonic(command.mnemonic.clone())
        })?;
        let mut raw = encode_command(command)?;
        if def.is_end_job() {
            if command.params.len() < def.params.len() {
                raw.extend(from_uint(0, END_JOB_BYTES));
            }
            let start = stream.len() + raw.len() - END_JOB_BYTES;
            checksum_offsets.push(start);
        } else if def.counts_toward_checksum() {
            checksum = checksum.wrapping_add(sum_bytes(&raw));
        }
        stream.extend(raw);
    }
    for start in checksum_offsets {
        stream[start..start + END_JOB_BYTES]
            .copy_from_slice(&from_uint(checksum, END_JOB_BYTES));
    }
    Ok(stream)
}

/// Encodes rpascript lines, applying the file checksum.
pub fn encode_script(lines: &[String]) -> Result<Vec<u8>, CodecError> {
    encode_commands(&parse_script(lines)?)
}

fn sum_bytes(bytes: &[u8]) -> u64 {
    bytes.iter().map(|byte| u64::from(*byte)).sum()
}

fn encode_param(param: Param, token: &str) -> Option<Vec<u8>> {
    let value = split_value(token);
    Some(match param {
        Param::Memory | Param::Index => encode_memory(value)?,
        Param::Rel => from_uint(parse_rel(value)?, 1),
        Param::Axis => from_uint(parse_axis(value)?, 1),
        Param::Bool => vec![u8::from(parse_bool(value)?)],
        Param::Dim14 => {
            from_int(round_half_even(parse_dim(value)? * 1000.0), 2)
        }
        Param::Dim35 => {
            from_int(round_half_even(parse_dim(value)? * 1000.0), 5)
        }
        Param::Power => {
            let percent = parse_float(strip_suffix(value, "%"))?;
            from_uint(
                round_half_even(percent * 16384.0 / 100.0).max(0) as u64,
                2,
            )
        }
        Param::Frequency => {
            let value =
                parse_float(strip_suffixes(value, &["KHz", "khz", "kHz"]))?;
            from_int(round_half_even(value * 1000.0), 5)
        }
        Param::Speed => {
            let value =
                parse_float(strip_suffixes(value, &["mm/S", "mm/s", "MM/S"]))?;
            from_int(round_half_even(value * 1000.0), 5)
        }
        Param::Time => {
            let value =
                parse_float(strip_suffixes(value, &["mS", "ms", "MS"]))?;
            from_int(round_half_even(value * 1000.0), 5)
        }
        Param::CString => {
            let mut bytes = unquote(value).as_bytes().to_vec();
            bytes.push(0);
            bytes
        }
        Param::String8 => encode_string8(unquote(value))?,
        Param::Color => {
            let rgb = parse_int(value)? as u64 & 0xFF_FFFF;
            from_uint(swap_red_blue(rgb), 5)
        }
        Param::CardId => from_uint(parse_card_id(value)?, 5),
        Param::Int7 => from_int(parse_int(value)?, 1),
        Param::Uint7 => from_uint(parse_int(value)? as u64, 1),
        Param::Int14 => from_int(parse_int(value)?, 2),
        Param::Uint14 => from_uint(parse_int(value)? as u64, 2),
        Param::Int35 => from_int(parse_int(value)?, 5),
        Param::Uint35 => from_uint(parse_int(value)? as u64, 5),
    })
}

fn split_value(token: &str) -> &str {
    let value = token.trim();
    let value = match value.split_once('=') {
        Some((_, rest)) => rest,
        None => value,
    };
    strip_label(value).trim()
}

fn strip_label(value: &str) -> &str {
    match value.split_once(':') {
        Some((label, rest)) if !label.is_empty() => {
            if label.chars().all(|ch| ch.is_ascii_alphabetic()) {
                return rest.trim_start();
            }
            value
        }
        _ => value,
    }
}

fn strip_suffix<'a>(value: &'a str, suffix: &str) -> &'a str {
    value.strip_suffix(suffix).unwrap_or(value)
}

fn strip_suffixes<'a>(value: &'a str, suffixes: &[&str]) -> &'a str {
    let lower = value.to_ascii_lowercase();
    for suffix in suffixes {
        if lower.ends_with(&suffix.to_ascii_lowercase()) {
            return &value[..value.len() - suffix.len()];
        }
    }
    value
}

fn unquote(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_uppercase().as_str() {
        "ON" | "TRUE" | "1" => Some(true),
        "OFF" | "FALSE" | "0" => Some(false),
        _ => None,
    }
}

fn parse_dim(value: &str) -> Option<f64> {
    let lower = value.to_ascii_lowercase();
    let clean = match lower.strip_suffix("mm") {
        Some(_) => &value[..value.len() - 2],
        None => value,
    };
    clean.trim().parse::<f64>().ok()
}

fn parse_float(value: &str) -> Option<f64> {
    value.trim().parse::<f64>().ok()
}

fn parse_int(value: &str) -> Option<i64> {
    let value = value.trim();
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        return i64::from_str_radix(hex, 16).ok();
    }
    if let Some(hex) = value.strip_prefix('#') {
        return i64::from_str_radix(hex, 16).ok();
    }
    value.parse::<i64>().ok()
}

fn parse_rel(value: &str) -> Option<u64> {
    match value.to_ascii_uppercase().as_str() {
        "MACHINE" => Some(0),
        "ANCHOR" => Some(1),
        "CURRENT" => Some(2),
        "SET_POINT" => Some(3),
        _ => parse_int(value).map(|value| value as u64),
    }
}

fn parse_axis(value: &str) -> Option<u64> {
    match value.to_ascii_uppercase().as_str() {
        "X" => Some(0),
        "Y" => Some(1),
        "Z" => Some(2),
        "U" => Some(3),
        _ => parse_int(value).map(|value| value as u64),
    }
}

fn parse_card_id(value: &str) -> Option<u64> {
    match value {
        "RDC6442S" => Some(0x6510_6510),
        "RDC8445S" => Some(0x9010_9010),
        _ => parse_int(value).map(|value| value as u64),
    }
}

fn encode_memory(value: &str) -> Option<Vec<u8>> {
    let address = match memory_address(value) {
        Some(address) => address,
        None => {
            let number = parse_int(value)?;
            u16::try_from(number).ok()?
        }
    };
    Some(vec![(address >> 8) as u8 & 0x7F, address as u8 & 0x7F])
}

pub fn get_setting(address: u16) -> Vec<u8> {
    vec![
        SETTING_PREFIX,
        0x00,
        (address >> 8) as u8 & 0x7F,
        address as u8 & 0x7F,
    ]
}

/// Builds a raw ``SET_SETTING`` command.
pub fn set_setting(address: u16, a: u64, b: u64) -> Vec<u8> {
    let mut command = vec![
        SETTING_PREFIX,
        0x01,
        (address >> 8) as u8 & 0x7F,
        address as u8 & 0x7F,
    ];
    command.extend(from_uint(a, 5));
    command.extend(from_uint(b, 5));
    command
}
