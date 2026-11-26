//! Ruida byte swizzling.
//!
//! Counterpart of ``ruida_pa.rpalib.rpa_swizzler``.

use crate::ruida::protocols::ruida::ruida_protocol::ACK;

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
