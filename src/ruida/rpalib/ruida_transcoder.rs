//! Ruida transcoder primitives.
//!
//! Rust counterpart of ``ruida_pa.rpalib.ruida_transcoder``: 7-bit
//! integer packing and the typed value encoders.

pub(crate) fn encode_string8(value: &str) -> Option<Vec<u8>> {
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

pub(crate) fn swap_red_blue(rgb: u64) -> u64 {
    ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF)
}

pub(crate) fn round_half_even(value: f64) -> i64 {
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
