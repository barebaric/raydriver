//! Ruida wire codec: rpascript commands, 7-bit value packing,
//! the XOR swizzle, packet checksums and reply framing.
//!
//! Ruida controllers speak a binary protocol built from 7-bit
//! values. A command starts with a prefix byte that keeps bit 7 set;
//! every following byte carries seven data bits. Dimensions travel
//! as signed micrometres, speeds/frequencies/times as thousandths,
//! and power as a 14-bit fraction of ``0x4000``. The byte stream is
//! obfuscated by a per-controller swizzle ("magic") before it goes
//! on the wire; UDP packets additionally carry a two-byte checksum
//! prefix and are acknowledged one packet at a time.
//!
//! The protocol tables themselves live in
//! :mod:`crate::ruida::protocol`; this module only encodes them into
//! bytes and decodes them back.

use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

pub use crate::ruida::protocol::{CommandDef, Param, ACK, ENQ, EOF, ERR, NAK};
use crate::ruida::protocol::{
    COMMANDS, END_JOB_BYTES, END_JOB_OPCODE, FILE_PREFIX, KEYPAD_PREFIX,
    MEMORY, SETTING_PREFIX,
};

/// A malformed script or parameter.
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

/// One parsed rpascript line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub mnemonic: String,
    pub params: Vec<String>,
}

static COMMAND_INDEX: OnceLock<HashMap<&'static str, &'static CommandDef>> =
    OnceLock::new();

static MEMORY_INDEX: OnceLock<HashMap<&'static str, u16>> = OnceLock::new();

/// Looks up a command by mnemonic.
///
/// The command table is grouped by protocol family; this index keeps
/// lookup independent of the table order.
pub fn lookup(mnemonic: &str) -> Option<&'static CommandDef> {
    COMMAND_INDEX
        .get_or_init(|| {
            COMMANDS
                .iter()
                .map(|entry| (entry.mnemonic, entry))
                .collect()
        })
        .get(mnemonic)
        .copied()
}

/// Looks up a memory address mnemonic.
pub fn memory_address(mnemonic: &str) -> Option<u16> {
    MEMORY_INDEX
        .get_or_init(|| MEMORY.iter().copied().collect())
        .get(mnemonic)
        .copied()
}

/// Parses rpascript lines into commands.
///
/// Comments (``#``, ``\#`` escaped), the ``= expected`` suffix of
/// replay scripts, and the analyzer's ``session``/``server``/
/// ``new_packet`` directives are dropped.
pub fn parse_script(lines: &[String]) -> Result<Vec<Command>, CodecError> {
    let mut commands = Vec::new();
    let mut in_block_comment = false;
    for line in lines {
        let (text, still_open) = strip_block_comment(line, in_block_comment);
        in_block_comment = still_open;
        let text = strip_line_comment(&text);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let mut tokens = text.split_whitespace();
        let Some(mnemonic) = tokens.next() else {
            continue;
        };
        let lower = mnemonic.to_ascii_lowercase();
        if matches!(lower.as_str(), "session" | "server" | "new_packet") {
            continue;
        }
        let mut params = Vec::new();
        for token in tokens {
            if token.starts_with('=') {
                break;
            }
            params.push(token.to_string());
        }
        commands.push(Command {
            mnemonic: mnemonic.to_string(),
            params,
        });
    }
    Ok(commands)
}

fn strip_block_comment(line: &str, mut open: bool) -> (String, bool) {
    if !open && !line.contains("\"\"\"") {
        return (line.to_string(), false);
    }
    let mut out = String::new();
    let mut rest = line;
    loop {
        if open {
            match rest.find("\"\"\"") {
                Some(end) => {
                    open = false;
                    rest = &rest[end + 3..];
                }
                None => break,
            }
        } else {
            match rest.find("\"\"\"") {
                Some(start) => {
                    out.push_str(&rest[..start]);
                    open = true;
                    rest = &rest[start + 3..];
                }
                None => {
                    out.push_str(rest);
                    break;
                }
            }
        }
    }
    (out, open)
}

fn strip_line_comment(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'#') {
            out.push('#');
            chars.next();
        } else if ch == '#' {
            break;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Encodes one command to raw (unswizzled) bytes.
pub fn encode_command(command: &Command) -> Result<Vec<u8>, CodecError> {
    let def = lookup(&command.mnemonic)
        .ok_or_else(|| CodecError::UnknownMnemonic(command.mnemonic.clone()))?;
    let mut raw = def.header();
    for (param, token) in def.params.iter().zip(&command.params) {
        raw.extend(encode_param(*param, token).ok_or_else(|| {
            CodecError::BadParam {
                mnemonic: command.mnemonic.clone(),
                value: token.clone(),
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

fn encode_string8(value: &str) -> Option<Vec<u8>> {
    let mut bytes = [0u8; 8];
    let source = value.as_bytes();
    let length = source.len().min(8);
    bytes[..length].copy_from_slice(&source[..length]);
    let mut out = from_uint(
        u64::from_be_bytes([
            0, 0, 0, 0, bytes[0], bytes[1], bytes[2], bytes[3],
        ]),
        5,
    );
    out.extend(from_uint(
        u64::from_be_bytes([
            0, 0, 0, 0, bytes[4], bytes[5], bytes[6], bytes[7],
        ]),
        5,
    ));
    Some(out)
}

fn swap_red_blue(rgb: u64) -> u64 {
    ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF)
}

fn round_half_even(value: f64) -> i64 {
    let truncated = value.trunc();
    if (value - truncated).abs() == 0.5 {
        let even = truncated as i64;
        if even % 2 == 0 {
            even
        } else {
            (truncated + value.signum()) as i64
        }
    } else {
        value.round() as i64
    }
}

/// Packs an unsigned integer into ``n`` 7-bit bytes, most
/// significant group first.
pub fn from_uint(value: u64, n: usize) -> Vec<u8> {
    (0..n)
        .rev()
        .map(|index| ((value >> (index * 7)) & 0x7F) as u8)
        .collect()
}

/// Packs a signed integer using the protocol's sign convention: the
/// top bit of the first byte is the sign, the data bits are two's
/// complement.
pub fn from_int(value: i64, n: usize) -> Vec<u8> {
    let mut data = if value < 0 {
        let mask = (1i64 << (n * 7 - 1)) - 1;
        mask + 1 + value
    } else {
        value
    };
    if data < 0 {
        data = 0;
    }
    let mut bytes = from_uint(data as u64, n);
    if value < 0 {
        bytes[0] |= 0x40;
    }
    bytes
}

/// Reads ``n`` 7-bit bytes as an unsigned integer.
pub fn to_uint(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(0, |value, byte| (value << 7) | u64::from(byte & 0x7F))
}

/// Reads ``n`` 7-bit bytes as a signed integer, including the
/// LightBurn sign-extension workaround.
pub fn to_int(bytes: &[u8]) -> i64 {
    let Some(first) = bytes.first().copied() else {
        return 0;
    };
    let mut first = first;
    if bytes.len() == 5 && first & 0x40 == 0 && first & 0x08 != 0 {
        first |= 0x70;
    }
    let mut value = 0i64;
    for (index, byte) in bytes.iter().enumerate() {
        let mut data = byte & 0x7F;
        if index == 0 {
            data = first & 0x3F;
        }
        value = (value << 7) | i64::from(data);
    }
    if first & 0x40 != 0 {
        let mask = (1i64 << (bytes.len() * 7 - 1)) - 1;
        value = -((!value & mask) + 1);
    }
    value
}

/// Forward-swizzles one byte with the controller's magic.
pub fn swizzle_byte(mut byte: u8, magic: u8) -> u8 {
    byte ^= byte >> 7;
    byte ^= byte << 7;
    byte ^= byte >> 7;
    byte ^= magic;
    byte.wrapping_add(1)
}

/// Reverses [`swizzle_byte`].
pub fn unswizzle_byte(mut byte: u8, magic: u8) -> u8 {
    byte = byte.wrapping_sub(1);
    byte ^= magic;
    byte ^= byte >> 7;
    byte ^= byte << 7;
    byte ^= byte >> 7;
    byte
}

/// Swizzles a byte slice.
pub fn swizzle(data: &[u8], magic: u8) -> Vec<u8> {
    data.iter().map(|byte| swizzle_byte(*byte, magic)).collect()
}

/// Unswizzles a byte slice.
pub fn unswizzle(data: &[u8], magic: u8) -> Vec<u8> {
    data.iter()
        .map(|byte| unswizzle_byte(*byte, magic))
        .collect()
}

/// Builds the table that maps a raw single-byte reply to the magic
/// that makes it an ACK.
pub fn ack_magic_table() -> [u8; 256] {
    let mut table = [0u8; 256];
    for magic in 0..=255u8 {
        table[usize::from(swizzle_byte(ACK, magic))] = magic;
    }
    table
}

/// Packages a raw payload for the wire: swizzle it and, for UDP,
/// prefix the checksum of the swizzled bytes.
pub fn package(payload: &[u8], magic: u8, udp: bool) -> Vec<u8> {
    let swizzled = swizzle(payload, magic);
    if !udp {
        return swizzled;
    }
    let checksum: u16 = swizzled
        .iter()
        .fold(0u16, |sum, byte| sum.wrapping_add(u16::from(*byte)));
    let mut packet = Vec::with_capacity(swizzled.len() + 2);
    packet.push((checksum >> 8) as u8);
    packet.push((checksum & 0xFF) as u8);
    packet.extend(swizzled);
    packet
}

/// A decoded ``GET_SETTING`` reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub address: u16,
    pub data: Vec<u8>,
}

/// Splits unswizzled reply bytes into replies.
///
/// A reply starts with ``0xDA`` and ends at the next byte with bit 7
/// set, because every value byte is 7-bit. Chunks shorter than nine
/// bytes are malformed and skipped; scanning continues.
pub fn unpack_replies(raw: &[u8]) -> Vec<Reply> {
    const MIN_REPLY_LEN: usize = 9;
    let mut replies = Vec::new();
    let mut index = 0;
    while index < raw.len() {
        if raw[index] != 0xDA {
            break;
        }
        let mut end = index + 1;
        while end < raw.len() && raw[end] & 0x80 == 0 {
            end += 1;
        }
        let chunk = &raw[index..end];
        if chunk.len() < MIN_REPLY_LEN {
            index = end;
            continue;
        }
        if chunk[1] != 0x01 {
            break;
        }
        replies.push(Reply {
            address: (u16::from(chunk[2]) << 8) | u16::from(chunk[3]),
            data: chunk[4..].to_vec(),
        });
        index = end;
    }
    replies
}

/// Builds a raw ``GET_SETTING`` command for an address.
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
