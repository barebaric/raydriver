//! Ruida protocol tables.
//!
//! This is a Rust translation of ``ruida_pa.protocols.ruida.ruida_protocol``:
//! the handshake bytes, command table (``CT``), memory table (``MT``) and
//! machine-status bits live here. Byte-level encoding and decoding lives
//! in :mod:`codec`; transports and sessions must refer to the names and
//! constants in this module instead of repeating raw protocol values.
//!
//! The command table is grouped by prefix family, matching the nested
//! dictionaries of the Python table. Lookup order is established by
//! :func:`crate::ruida::codec::lookup`, so the source order can remain
//! protocol-oriented.

/// Controller accepted a packet.
pub const ACK: u8 = 0xCC;
/// Controller rejected a packet.
pub const ERR: u8 = 0xCD;
/// Keep-alive; reply with ENQ or ACK.
pub const ENQ: u8 = 0xCE;
/// Checksum mismatch; resend the packet.
pub const NAK: u8 = 0xCF;
/// End-of-file marker; part of the file checksum.
pub const EOF: u8 = 0xD7;

pub const KEYPAD_PREFIX: u8 = 0xA7;
pub const SETTING_PREFIX: u8 = 0xDA;
pub const FILE_PREFIX: u8 = 0xE5;
pub const END_JOB_OPCODE: u8 = 0x05;
pub const END_JOB_BYTES: usize = 5;

/// How one rpascript parameter is encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Param {
    Int7,
    Uint7,
    Int14,
    Uint14,
    Int35,
    Uint35,
    Dim14,
    Dim35,
    Power,
    Frequency,
    Speed,
    Time,
    Bool,
    CString,
    String8,
    Color,
    Memory,
    Index,
    CardId,
    Rel,
    Axis,
}

/// One flattened entry of the Ruida command table.
pub struct CommandDef {
    pub mnemonic: &'static str,
    pub prefix: u8,
    pub mid: Option<u8>,
    pub inner: Option<u8>,
    pub params: &'static [Param],
}

/// Command table, grouped by prefix family as in the Python ``CT`` table.
pub(crate) static COMMANDS: &[CommandDef] = &[
    // Prefix 0x80 — Move commands
    CommandDef {
        mnemonic: "MOVE_FAR_X",
        prefix: 0x80,
        mid: Some(0x00),
        inner: None,
        params: &[Param::Dim35],
    },
    CommandDef {
        mnemonic: "MOVE_FAR_Y",
        prefix: 0x80,
        mid: Some(0x01),
        inner: None,
        params: &[Param::Dim35],
    },
    CommandDef {
        mnemonic: "MOVE_FAR_Z",
        prefix: 0x80,
        mid: Some(0x03),
        inner: None,
        params: &[Param::Dim35],
    },
    CommandDef {
        mnemonic: "MOVE_FAR_U",
        prefix: 0x80,
        mid: Some(0x02),
        inner: None,
        params: &[Param::Dim35],
    },
    // Prefix 0x88 — Move commands
    CommandDef {
        mnemonic: "MOVE_FAR_XY",
        prefix: 0x88,
        mid: None,
        inner: None,
        params: &[Param::Dim35, Param::Dim35],
    },
    // Prefix 0x89 — Move commands
    CommandDef {
        mnemonic: "MOVE_NEAR_XY",
        prefix: 0x89,
        mid: None,
        inner: None,
        params: &[Param::Dim14, Param::Dim14],
    },
    // Prefix 0x8A — Move commands
    CommandDef {
        mnemonic: "MOVE_NEAR_X",
        prefix: 0x8A,
        mid: None,
        inner: None,
        params: &[Param::Dim14],
    },
    // Prefix 0x8B — Move commands
    CommandDef {
        mnemonic: "MOVE_NEAR_Y",
        prefix: 0x8B,
        mid: None,
        inner: None,
        params: &[Param::Dim14],
    },
    // Prefix 0xA0 — Axis move commands
    CommandDef {
        mnemonic: "MOVE_FAR_A",
        prefix: 0xA0,
        mid: Some(0x00),
        inner: None,
        params: &[Param::Dim35],
    },
    CommandDef {
        mnemonic: "MOVE_FAR_U",
        prefix: 0xA0,
        mid: Some(0x08),
        inner: None,
        params: &[Param::Dim35],
    },
    // Prefix 0xA8 — Cut commands
    CommandDef {
        mnemonic: "CUT_FAR_XY",
        prefix: 0xA8,
        mid: None,
        inner: None,
        params: &[Param::Dim35, Param::Dim35],
    },
    // Prefix 0xA9 — Cut commands
    CommandDef {
        mnemonic: "CUT_NEAR_XY",
        prefix: 0xA9,
        mid: None,
        inner: None,
        params: &[Param::Dim14, Param::Dim14],
    },
    // Prefix 0xAA — Cut commands
    CommandDef {
        mnemonic: "CUT_NEAR_X",
        prefix: 0xAA,
        mid: None,
        inner: None,
        params: &[Param::Dim14],
    },
    // Prefix 0xAB — Cut commands
    CommandDef {
        mnemonic: "CUT_NEAR_Y",
        prefix: 0xAB,
        mid: None,
        inner: None,
        params: &[Param::Dim14],
    },
    // Prefix 0xC0 — Power, speed and timing
    CommandDef {
        mnemonic: "IMD_POWER_2",
        prefix: 0xC0,
        mid: None,
        inner: None,
        params: &[Param::Power],
    },
    // Prefix 0xC1 — Power, speed and timing
    CommandDef {
        mnemonic: "END_POWER_2",
        prefix: 0xC1,
        mid: None,
        inner: None,
        params: &[Param::Power],
    },
    // Prefix 0xC2 — Power, speed and timing
    CommandDef {
        mnemonic: "IMD_POWER_3",
        prefix: 0xC2,
        mid: None,
        inner: None,
        params: &[Param::Power],
    },
    // Prefix 0xC3 — Power, speed and timing
    CommandDef {
        mnemonic: "END_POWER_3",
        prefix: 0xC3,
        mid: None,
        inner: None,
        params: &[Param::Power],
    },
    // Prefix 0xC4 — Power, speed and timing
    CommandDef {
        mnemonic: "IMD_POWER_4",
        prefix: 0xC4,
        mid: None,
        inner: None,
        params: &[Param::Power],
    },
    // Prefix 0xC5 — Power, speed and timing
    CommandDef {
        mnemonic: "END_POWER_4",
        prefix: 0xC5,
        mid: None,
        inner: None,
        params: &[Param::Power],
    },
    // Prefix 0xC6 — Power, speed and timing
    CommandDef {
        mnemonic: "MIN_POWER_1",
        prefix: 0xC6,
        mid: Some(0x01),
        inner: None,
        params: &[Param::Power],
    },
    CommandDef {
        mnemonic: "MAX_POWER_1",
        prefix: 0xC6,
        mid: Some(0x02),
        inner: None,
        params: &[Param::Power],
    },
    CommandDef {
        mnemonic: "MIN_POWER_3",
        prefix: 0xC6,
        mid: Some(0x05),
        inner: None,
        params: &[Param::Power],
    },
    CommandDef {
        mnemonic: "MAX_POWER_3",
        prefix: 0xC6,
        mid: Some(0x06),
        inner: None,
        params: &[Param::Power],
    },
    CommandDef {
        mnemonic: "MIN_POWER_4",
        prefix: 0xC6,
        mid: Some(0x07),
        inner: None,
        params: &[Param::Power],
    },
    CommandDef {
        mnemonic: "MAX_POWER_4",
        prefix: 0xC6,
        mid: Some(0x08),
        inner: None,
        params: &[Param::Power],
    },
    CommandDef {
        mnemonic: "CUT_PIERCE",
        prefix: 0xC6,
        mid: Some(0x10),
        inner: None,
        params: &[Param::Time],
    },
    CommandDef {
        mnemonic: "ADD_CUT_DELAY",
        prefix: 0xC6,
        mid: Some(0x11),
        inner: None,
        params: &[Param::Time],
    },
    CommandDef {
        mnemonic: "LASER_ON_DELAY",
        prefix: 0xC6,
        mid: Some(0x12),
        inner: None,
        params: &[Param::Time],
    },
    CommandDef {
        mnemonic: "LASER_OFF_DELAY",
        prefix: 0xC6,
        mid: Some(0x13),
        inner: None,
        params: &[Param::Time],
    },
    CommandDef {
        mnemonic: "LASER_ON_DELAY2",
        prefix: 0xC6,
        mid: Some(0x15),
        inner: None,
        params: &[Param::Time],
    },
    CommandDef {
        mnemonic: "LASER_OFF_DELAY2",
        prefix: 0xC6,
        mid: Some(0x16),
        inner: None,
        params: &[Param::Time],
    },
    CommandDef {
        mnemonic: "MIN_POWER_2",
        prefix: 0xC6,
        mid: Some(0x21),
        inner: None,
        params: &[Param::Power],
    },
    CommandDef {
        mnemonic: "MAX_POWER_2",
        prefix: 0xC6,
        mid: Some(0x22),
        inner: None,
        params: &[Param::Power],
    },
    CommandDef {
        mnemonic: "LAYER_MIN_POWER_1",
        prefix: 0xC6,
        mid: Some(0x31),
        inner: None,
        params: &[Param::Int7, Param::Power],
    },
    CommandDef {
        mnemonic: "LAYER_MAX_POWER_1",
        prefix: 0xC6,
        mid: Some(0x32),
        inner: None,
        params: &[Param::Int7, Param::Power],
    },
    CommandDef {
        mnemonic: "LAYER_MIN_POWER_3",
        prefix: 0xC6,
        mid: Some(0x35),
        inner: None,
        params: &[Param::Int7, Param::Power],
    },
    CommandDef {
        mnemonic: "LAYER_MAX_POWER_3",
        prefix: 0xC6,
        mid: Some(0x36),
        inner: None,
        params: &[Param::Int7, Param::Power],
    },
    CommandDef {
        mnemonic: "LAYER_MIN_POWER_4",
        prefix: 0xC6,
        mid: Some(0x37),
        inner: None,
        params: &[Param::Int7, Param::Power],
    },
    CommandDef {
        mnemonic: "LAYER_MAX_POWER_4",
        prefix: 0xC6,
        mid: Some(0x38),
        inner: None,
        params: &[Param::Int7, Param::Power],
    },
    CommandDef {
        mnemonic: "LAYER_MIN_POWER_2",
        prefix: 0xC6,
        mid: Some(0x41),
        inner: None,
        params: &[Param::Int7, Param::Power],
    },
    CommandDef {
        mnemonic: "LAYER_MAX_POWER_2",
        prefix: 0xC6,
        mid: Some(0x42),
        inner: None,
        params: &[Param::Int7, Param::Power],
    },
    CommandDef {
        mnemonic: "THROUGH_POWER_1",
        prefix: 0xC6,
        mid: Some(0x50),
        inner: None,
        params: &[Param::Power],
    },
    CommandDef {
        mnemonic: "THROUGH_POWER_2",
        prefix: 0xC6,
        mid: Some(0x51),
        inner: None,
        params: &[Param::Power],
    },
    CommandDef {
        mnemonic: "THROUGH_POWER_3",
        prefix: 0xC6,
        mid: Some(0x55),
        inner: None,
        params: &[Param::Power],
    },
    CommandDef {
        mnemonic: "THROUGH_POWER_4",
        prefix: 0xC6,
        mid: Some(0x56),
        inner: None,
        params: &[Param::Power],
    },
    CommandDef {
        mnemonic: "LAYER_FREQUENCY",
        prefix: 0xC6,
        mid: Some(0x60),
        inner: None,
        params: &[Param::Int7, Param::Int7, Param::Frequency],
    },
    // Prefix 0xC7 — Power, speed and timing
    CommandDef {
        mnemonic: "IMD_POWER_1",
        prefix: 0xC7,
        mid: None,
        inner: None,
        params: &[Param::Power],
    },
    // Prefix 0xC8 — Power, speed and timing
    CommandDef {
        mnemonic: "END_POWER_1",
        prefix: 0xC8,
        mid: None,
        inner: None,
        params: &[Param::Power],
    },
    // Prefix 0xC9 — Speed commands
    CommandDef {
        mnemonic: "SPEED_LASER_1",
        prefix: 0xC9,
        mid: Some(0x02),
        inner: None,
        params: &[Param::Speed],
    },
    CommandDef {
        mnemonic: "SPEED_AXIS",
        prefix: 0xC9,
        mid: Some(0x03),
        inner: None,
        params: &[Param::Speed],
    },
    CommandDef {
        mnemonic: "CUT_SPEED_LASER_1",
        prefix: 0xC9,
        mid: Some(0x04),
        inner: None,
        params: &[Param::Int7, Param::Speed],
    },
    CommandDef {
        mnemonic: "FORCE_ENG_SPEED",
        prefix: 0xC9,
        mid: Some(0x05),
        inner: None,
        params: &[Param::Speed],
    },
    CommandDef {
        mnemonic: "SPEED_AXIS_MOVE",
        prefix: 0xC9,
        mid: Some(0x06),
        inner: None,
        params: &[Param::Speed],
    },
    // Prefix 0xCA — Device, laser, overscan and job settings
    CommandDef {
        mnemonic: "OVERSCAN_OFF",
        prefix: 0xCA,
        mid: Some(0x01),
        inner: Some(0x00),
        params: &[],
    },
    CommandDef {
        mnemonic: "OVERSCAN_H_BI",
        prefix: 0xCA,
        mid: Some(0x01),
        inner: Some(0x01),
        params: &[],
    },
    CommandDef {
        mnemonic: "OVERSCAN_H_UNI",
        prefix: 0xCA,
        mid: Some(0x01),
        inner: Some(0x02),
        params: &[],
    },
    CommandDef {
        mnemonic: "OVERSCAN_V_BI",
        prefix: 0xCA,
        mid: Some(0x01),
        inner: Some(0x03),
        params: &[],
    },
    CommandDef {
        mnemonic: "OVERSCAN_V_UNI",
        prefix: 0xCA,
        mid: Some(0x01),
        inner: Some(0x04),
        params: &[],
    },
    CommandDef {
        mnemonic: "OVERSCAN_DIAGONAL",
        prefix: 0xCA,
        mid: Some(0x01),
        inner: Some(0x05),
        params: &[],
    },
    CommandDef {
        mnemonic: "LASER_DEVICE_0",
        prefix: 0xCA,
        mid: Some(0x01),
        inner: Some(0x10),
        params: &[],
    },
    CommandDef {
        mnemonic: "LASER_DEVICE_1",
        prefix: 0xCA,
        mid: Some(0x01),
        inner: Some(0x11),
        params: &[],
    },
    CommandDef {
        mnemonic: "AIR_ASSIST_OFF",
        prefix: 0xCA,
        mid: Some(0x01),
        inner: Some(0x12),
        params: &[],
    },
    CommandDef {
        mnemonic: "AIR_ASSIST_ON",
        prefix: 0xCA,
        mid: Some(0x01),
        inner: Some(0x13),
        params: &[],
    },
    CommandDef {
        mnemonic: "DB_HEAD",
        prefix: 0xCA,
        mid: Some(0x01),
        inner: Some(0x14),
        params: &[],
    },
    CommandDef {
        mnemonic: "EN_LASER_2_OFFSET_0",
        prefix: 0xCA,
        mid: Some(0x01),
        inner: Some(0x30),
        params: &[],
    },
    CommandDef {
        mnemonic: "EN_LASER_2_OFFSET_1",
        prefix: 0xCA,
        mid: Some(0x01),
        inner: Some(0x31),
        params: &[],
    },
    CommandDef {
        mnemonic: "OVERSCAN_5",
        prefix: 0xCA,
        mid: Some(0x01),
        inner: Some(0x55),
        params: &[],
    },
    CommandDef {
        mnemonic: "SELECT_LAYER",
        prefix: 0xCA,
        mid: Some(0x02),
        inner: None,
        params: &[Param::Int7],
    },
    CommandDef {
        mnemonic: "EN_LASER_TUBE_START",
        prefix: 0xCA,
        mid: Some(0x03),
        inner: None,
        params: &[Param::Bool],
    },
    CommandDef {
        mnemonic: "X_SIGN_MAP",
        prefix: 0xCA,
        mid: Some(0x04),
        inner: None,
        params: &[Param::Int7],
    },
    CommandDef {
        mnemonic: "DEFAULT_COLOR",
        prefix: 0xCA,
        mid: Some(0x05),
        inner: None,
        params: &[Param::Color],
    },
    CommandDef {
        mnemonic: "LAYER_COLOR",
        prefix: 0xCA,
        mid: Some(0x06),
        inner: None,
        params: &[Param::Int7, Param::Color],
    },
    CommandDef {
        mnemonic: "EN_EX_IO",
        prefix: 0xCA,
        mid: Some(0x10),
        inner: None,
        params: &[Param::Int7],
    },
    CommandDef {
        mnemonic: "LAST_LAYER",
        prefix: 0xCA,
        mid: Some(0x22),
        inner: None,
        params: &[Param::Int7],
    },
    CommandDef {
        mnemonic: "U_FILE_ID",
        prefix: 0xCA,
        mid: Some(0x30),
        inner: None,
        params: &[Param::Uint14],
    },
    CommandDef {
        mnemonic: "ZU_MAP",
        prefix: 0xCA,
        mid: Some(0x40),
        inner: None,
        params: &[Param::Int7],
    },
    CommandDef {
        mnemonic: "LAYER_ATTRIBUTES",
        prefix: 0xCA,
        mid: Some(0x41),
        inner: None,
        params: &[Param::Int7, Param::Uint7],
    },
    // Prefix 0xCE — Handshake
    CommandDef {
        mnemonic: "ENQ",
        prefix: 0xCE,
        mid: None,
        inner: None,
        params: &[],
    },
    // Prefix 0xD0 — Flow control
    CommandDef {
        mnemonic: "SKIP",
        prefix: 0xD0,
        mid: Some(0x29),
        inner: None,
        params: &[],
    },
    // Prefix 0xD7 — End of file
    CommandDef {
        mnemonic: "EOF",
        prefix: 0xD7,
        mid: None,
        inner: None,
        params: &[],
    },
    // Prefix 0xD8 — Job control, homing and reference points
    CommandDef {
        mnemonic: "START_JOB",
        prefix: 0xD8,
        mid: Some(0x00),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "STOP_JOB",
        prefix: 0xD8,
        mid: Some(0x01),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "PAUSE_JOB",
        prefix: 0xD8,
        mid: Some(0x02),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "RESUME_JOB",
        prefix: 0xD8,
        mid: Some(0x03),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "REF_POINT_MACHINE",
        prefix: 0xD8,
        mid: Some(0x10),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "REF_POINT_ORIGIN",
        prefix: 0xD8,
        mid: Some(0x11),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "REF_POINT_CURRENT",
        prefix: 0xD8,
        mid: Some(0x12),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "KEYDOWN_X_LEFT",
        prefix: 0xD8,
        mid: Some(0x20),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "KEYDOWN_X_RIGHT",
        prefix: 0xD8,
        mid: Some(0x21),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "KEYDOWN_Y_TOP",
        prefix: 0xD8,
        mid: Some(0x22),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "KEYDOWN_Y_BOTTOM",
        prefix: 0xD8,
        mid: Some(0x23),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "KEYDOWN_Z_UP",
        prefix: 0xD8,
        mid: Some(0x24),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "KEYDOWN_Z_DOWN",
        prefix: 0xD8,
        mid: Some(0x25),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "KEYDOWN_U_FORWARD",
        prefix: 0xD8,
        mid: Some(0x26),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "KEYDOWN_U_BACKWARDS",
        prefix: 0xD8,
        mid: Some(0x27),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "HOME_XY",
        prefix: 0xD8,
        mid: Some(0x2A),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "HOME_Z",
        prefix: 0xD8,
        mid: Some(0x2C),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "HOME_U",
        prefix: 0xD8,
        mid: Some(0x2D),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "FOCUS_Z",
        prefix: 0xD8,
        mid: Some(0x2E),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "KEYUP_LEFT",
        prefix: 0xD8,
        mid: Some(0x30),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "KEYUP_RIGHT",
        prefix: 0xD8,
        mid: Some(0x31),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "KEYUP_Y_TOP",
        prefix: 0xD8,
        mid: Some(0x32),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "KEYUP_Y_BOTTOM",
        prefix: 0xD8,
        mid: Some(0x33),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "KEYUP_Z_UP",
        prefix: 0xD8,
        mid: Some(0x34),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "KEYUP_Z_DOWN",
        prefix: 0xD8,
        mid: Some(0x35),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "KEYUP_U_FORWARD",
        prefix: 0xD8,
        mid: Some(0x36),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "KEYUP_U_BACKWARDS",
        prefix: 0xD8,
        mid: Some(0x37),
        inner: None,
        params: &[],
    },
    // Prefix 0xD9 — Jog commands
    CommandDef {
        mnemonic: "JOG_X",
        prefix: 0xD9,
        mid: Some(0x00),
        inner: None,
        params: &[Param::Rel, Param::Dim35],
    },
    CommandDef {
        mnemonic: "JOG_Y",
        prefix: 0xD9,
        mid: Some(0x01),
        inner: None,
        params: &[Param::Rel, Param::Dim35],
    },
    CommandDef {
        mnemonic: "JOG_Z",
        prefix: 0xD9,
        mid: Some(0x02),
        inner: None,
        params: &[Param::Rel, Param::Dim35],
    },
    CommandDef {
        mnemonic: "JOG_U",
        prefix: 0xD9,
        mid: Some(0x03),
        inner: None,
        params: &[Param::Rel, Param::Dim35],
    },
    CommandDef {
        mnemonic: "JOG_FEED_AXIS",
        prefix: 0xD9,
        mid: Some(0x0F),
        inner: None,
        params: &[Param::Rel],
    },
    CommandDef {
        mnemonic: "JOG_XY",
        prefix: 0xD9,
        mid: Some(0x10),
        inner: None,
        params: &[Param::Rel, Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "JOG_XYU",
        prefix: 0xD9,
        mid: Some(0x30),
        inner: None,
        params: &[Param::Rel, Param::Dim35, Param::Dim35, Param::Dim35],
    },
    // Prefix 0xDA — Memory settings
    CommandDef {
        mnemonic: "GET_SETTING",
        prefix: 0xDA,
        mid: Some(0x00),
        inner: None,
        params: &[Param::Memory],
    },
    CommandDef {
        mnemonic: "SET_SETTING",
        prefix: 0xDA,
        mid: Some(0x01),
        inner: None,
        params: &[Param::Memory, Param::Uint35, Param::Uint35],
    },
    CommandDef {
        mnemonic: "GET_UNKNOWN",
        prefix: 0xDA,
        mid: Some(0x05),
        inner: None,
        params: &[Param::Index, Param::Uint35],
    },
    // Prefix 0xE5 — File and job commands
    CommandDef {
        mnemonic: "DOCUMENT_FILE_UPLOAD",
        prefix: 0xE5,
        mid: Some(0x00),
        inner: None,
        params: &[Param::Uint14, Param::Uint35, Param::Uint35],
    },
    CommandDef {
        mnemonic: "DOCUMENT_FILE_END",
        prefix: 0xE5,
        mid: Some(0x02),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "END_JOB",
        prefix: 0xE5,
        mid: Some(0x05),
        inner: None,
        params: &[Param::Uint35],
    },
    // Prefix 0xE6 — Document layout
    CommandDef {
        mnemonic: "SET_ABSOLUTE",
        prefix: 0xE6,
        mid: Some(0x01),
        inner: None,
        params: &[],
    },
    // Prefix 0xE7 — Document layout
    CommandDef {
        mnemonic: "BLOCK_END",
        prefix: 0xE7,
        mid: Some(0x00),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "SET_FILE_NAME",
        prefix: 0xE7,
        mid: Some(0x01),
        inner: None,
        params: &[Param::CString],
    },
    CommandDef {
        mnemonic: "JOB_TOP_RIGHT",
        prefix: 0xE7,
        mid: Some(0x03),
        inner: None,
        params: &[Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "JOB_COPIES",
        prefix: 0xE7,
        mid: Some(0x04),
        inner: None,
        params: &[Param::Uint14, Param::Uint14, Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "ARRAY_DIRECTION",
        prefix: 0xE7,
        mid: Some(0x05),
        inner: None,
        params: &[Param::Int7],
    },
    CommandDef {
        mnemonic: "FEED_REPEAT",
        prefix: 0xE7,
        mid: Some(0x06),
        inner: None,
        params: &[Param::Uint35, Param::Uint35],
    },
    CommandDef {
        mnemonic: "JOB_BOTTOM_LEFT",
        prefix: 0xE7,
        mid: Some(0x07),
        inner: None,
        params: &[Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "ARRAY_COPIES",
        prefix: 0xE7,
        mid: Some(0x08),
        inner: None,
        params: &[Param::Uint14, Param::Uint14, Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "FEED_LENGTH",
        prefix: 0xE7,
        mid: Some(0x09),
        inner: None,
        params: &[Param::Int35],
    },
    CommandDef {
        mnemonic: "FEED_INFO",
        prefix: 0xE7,
        mid: Some(0x0A),
        inner: None,
        params: &[Param::Int35],
    },
    CommandDef {
        mnemonic: "ARRAY_EN_MIRROR_CUT",
        prefix: 0xE7,
        mid: Some(0x0B),
        inner: None,
        params: &[Param::Uint7],
    },
    CommandDef {
        mnemonic: "ARRAY_TOP_RIGHT",
        prefix: 0xE7,
        mid: Some(0x13),
        inner: None,
        params: &[Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "ARRAY_BOTTOM_LEFT",
        prefix: 0xE7,
        mid: Some(0x17),
        inner: None,
        params: &[Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "ARRAY_ADD",
        prefix: 0xE7,
        mid: Some(0x23),
        inner: None,
        params: &[Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "ARRAY_MIRROR",
        prefix: 0xE7,
        mid: Some(0x24),
        inner: None,
        params: &[Param::Uint7],
    },
    CommandDef {
        mnemonic: "UNKNOWN_E732",
        prefix: 0xE7,
        mid: Some(0x32),
        inner: None,
        params: &[Param::Uint35, Param::Uint35],
    },
    CommandDef {
        mnemonic: "BLOCK_X_SIZE",
        prefix: 0xE7,
        mid: Some(0x35),
        inner: None,
        params: &[Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "SET_FILE_EMPTY",
        prefix: 0xE7,
        mid: Some(0x36),
        inner: None,
        params: &[Param::Uint7],
    },
    CommandDef {
        mnemonic: "ARRAY_EVEN_DISTANCE",
        prefix: 0xE7,
        mid: Some(0x37),
        inner: None,
        params: &[Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "SET_FEED_AUTO_PAUSE",
        prefix: 0xE7,
        mid: Some(0x38),
        inner: None,
        params: &[Param::Bool],
    },
    CommandDef {
        mnemonic: "UNION_BLOCK_PROPERTY",
        prefix: 0xE7,
        mid: Some(0x3A),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "DOCUMENT_TOP_RIGHT",
        prefix: 0xE7,
        mid: Some(0x50),
        inner: None,
        params: &[Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "DOCUMENT_BOTTOM_LEFT",
        prefix: 0xE7,
        mid: Some(0x51),
        inner: None,
        params: &[Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "LAYER_TOP_RIGHT",
        prefix: 0xE7,
        mid: Some(0x52),
        inner: None,
        params: &[Param::Int7, Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "LAYER_BOTTOM_LEFT",
        prefix: 0xE7,
        mid: Some(0x53),
        inner: None,
        params: &[Param::Int7, Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "PEN_OFFSET_AXIS",
        prefix: 0xE7,
        mid: Some(0x54),
        inner: None,
        params: &[Param::Axis, Param::Dim35],
    },
    CommandDef {
        mnemonic: "LAYER_OFFSET_AXIS",
        prefix: 0xE7,
        mid: Some(0x55),
        inner: None,
        params: &[Param::Axis, Param::Dim35],
    },
    CommandDef {
        mnemonic: "SET_CURRENT_ELEMENT_INDEX",
        prefix: 0xE7,
        mid: Some(0x60),
        inner: None,
        params: &[Param::Uint7],
    },
    CommandDef {
        mnemonic: "LAYER_EX_TOP_RIGHT",
        prefix: 0xE7,
        mid: Some(0x61),
        inner: None,
        params: &[Param::Int7, Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "LAYER_EX_BOTTOM_LEFT",
        prefix: 0xE7,
        mid: Some(0x62),
        inner: None,
        params: &[Param::Int7, Param::Dim35, Param::Dim35],
    },
    // Prefix 0xE8 — Document layout
    CommandDef {
        mnemonic: "DELETE_DOCUMENT",
        prefix: 0xE8,
        mid: Some(0x00),
        inner: None,
        params: &[Param::Uint35, Param::Uint35],
    },
    CommandDef {
        mnemonic: "DOCUMENT_NUMBER",
        prefix: 0xE8,
        mid: Some(0x01),
        inner: None,
        params: &[Param::Uint14],
    },
    CommandDef {
        mnemonic: "FILE_TRANSFER",
        prefix: 0xE8,
        mid: Some(0x02),
        inner: None,
        params: &[],
    },
    CommandDef {
        mnemonic: "SELECT_DOCUMENT",
        prefix: 0xE8,
        mid: Some(0x03),
        inner: None,
        params: &[Param::Uint7],
    },
    CommandDef {
        mnemonic: "CALCULATE_DOCUMENT_TIME",
        prefix: 0xE8,
        mid: Some(0x04),
        inner: None,
        params: &[],
    },
    // Prefix 0xEA — Document layout
    CommandDef {
        mnemonic: "ARRAY_START",
        prefix: 0xEA,
        mid: None,
        inner: None,
        params: &[Param::Uint7],
    },
    // Prefix 0xEB — Document layout
    CommandDef {
        mnemonic: "ARRAY_END",
        prefix: 0xEB,
        mid: None,
        inner: None,
        params: &[],
    },
    // Prefix 0xF0 — Element and layer layout
    CommandDef {
        mnemonic: "REF_POINT_SET",
        prefix: 0xF0,
        mid: None,
        inner: None,
        params: &[],
    },
    // Prefix 0xF1 — Element and layer layout
    CommandDef {
        mnemonic: "ELEMENT_MAX_INDEX",
        prefix: 0xF1,
        mid: Some(0x00),
        inner: None,
        params: &[Param::Uint7],
    },
    CommandDef {
        mnemonic: "ELEMENT_NAME_MAX_INDEX",
        prefix: 0xF1,
        mid: Some(0x01),
        inner: None,
        params: &[Param::Uint7],
    },
    CommandDef {
        mnemonic: "ENABLE_BLOCK_CUTTING",
        prefix: 0xF1,
        mid: Some(0x02),
        inner: None,
        params: &[Param::Bool],
    },
    CommandDef {
        mnemonic: "DISPLAY_OFFSET",
        prefix: 0xF1,
        mid: Some(0x03),
        inner: None,
        params: &[Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "FEED_AUTO_CALC",
        prefix: 0xF1,
        mid: Some(0x04),
        inner: None,
        params: &[Param::Uint7],
    },
    // Prefix 0xF2 — Element and layer layout
    CommandDef {
        mnemonic: "ELEMENT_INDEX",
        prefix: 0xF2,
        mid: Some(0x00),
        inner: None,
        params: &[Param::Uint7],
    },
    CommandDef {
        mnemonic: "ELEMENT_NAME_INDEX",
        prefix: 0xF2,
        mid: Some(0x01),
        inner: None,
        params: &[Param::Uint7],
    },
    CommandDef {
        mnemonic: "ELEMENT_NAME",
        prefix: 0xF2,
        mid: Some(0x02),
        inner: None,
        params: &[Param::String8],
    },
    CommandDef {
        mnemonic: "ELEMENT_ARRAY_TOP_RIGHT",
        prefix: 0xF2,
        mid: Some(0x03),
        inner: None,
        params: &[Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "ELEMENT_ARRAY_BOTTOM_LEFT",
        prefix: 0xF2,
        mid: Some(0x04),
        inner: None,
        params: &[Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "ELEMENT_COPIES",
        prefix: 0xF2,
        mid: Some(0x05),
        inner: None,
        params: &[Param::Uint14, Param::Uint14, Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "ELEMENT_ARRAY_ADD",
        prefix: 0xF2,
        mid: Some(0x06),
        inner: None,
        params: &[Param::Dim35, Param::Dim35],
    },
    CommandDef {
        mnemonic: "ELEMENT_ARRAY_MIRROR",
        prefix: 0xF2,
        mid: Some(0x07),
        inner: None,
        params: &[Param::Uint7],
    },
];

/// Machine-status word bits reported through ``MEM_MACHINE_STATUS``.
pub const MACHINE_STATUS_RUNNING: u32 = 0x0000_0001;
pub const MACHINE_STATUS_PAUSED: u32 = 0x0000_0002;
pub const MACHINE_STATUS_MOVING: u32 = 0x0100_0000;

/// Single-bit flags in ``MEM_MACHINE_FEATURES``.
pub const MACHINE_FEATURE_FOCUS: u32 = 0x0001;
pub const MACHINE_FEATURE_Z_RETURN_DOCKING: u32 = 0x0008;

/// Multi-bit air-assist mode field in ``MEM_MACHINE_FEATURES``.
pub const MACHINE_FEATURE_AIR_ASSIST_MASK: u32 = 0x0600;
pub const MACHINE_FEATURE_AIR_ASSIST_SHIFT: u32 = 9;

/// Decoded machine features.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MachineFeatures {
    pub focus: bool,
    pub z_return_docking: bool,
    pub air_assist_mode: u8,
}

impl MachineFeatures {
    pub fn from_bits(bits: u32) -> Self {
        Self {
            focus: bits & MACHINE_FEATURE_FOCUS != 0,
            z_return_docking: bits & MACHINE_FEATURE_Z_RETURN_DOCKING != 0,
            air_assist_mode: ((bits & MACHINE_FEATURE_AIR_ASSIST_MASK)
                >> MACHINE_FEATURE_AIR_ASSIST_SHIFT)
                as u8,
        }
    }
}

/// A controller register the session polls and decodes.
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusRegister {
    MachineStatus = 0x0400,
    PositionX = 0x0421,
    PositionY = 0x0431,
    PositionZ = 0x0441,
    PositionU = 0x0451,
    CardId = 0x057E,
    MainboardVersion = 0x057F,
    BedSizeX = 0x0026,
    BedSizeY = 0x0036,
    MachineFeatures = 0x030F,
}

impl StatusRegister {
    pub const ALL: &[Self] = &[
        Self::MachineStatus,
        Self::PositionX,
        Self::PositionY,
        Self::PositionZ,
        Self::PositionU,
        Self::CardId,
        Self::MainboardVersion,
        Self::BedSizeX,
        Self::BedSizeY,
        Self::MachineFeatures,
    ];

    /// Registers queried once after the ping handshake.
    pub const IDENTITY: &[Self] = &[
        Self::BedSizeX,
        Self::BedSizeY,
        Self::MachineFeatures,
        Self::MainboardVersion,
    ];

    /// Registers polled while the session is connected.
    pub const STATUS_POLL: &[Self] = &[
        Self::MachineStatus,
        Self::PositionX,
        Self::PositionY,
        Self::PositionZ,
        Self::PositionU,
    ];

    pub fn address(self) -> u16 {
        self as u16
    }

    pub fn mnemonic(self) -> &'static str {
        match self {
            Self::MachineStatus => "MEM_MACHINE_STATUS",
            Self::PositionX => "MEM_CURRENT_POSITION_X",
            Self::PositionY => "MEM_CURRENT_POSITION_Y",
            Self::PositionZ => "MEM_CURRENT_POSITION_Z",
            Self::PositionU => "MEM_CURRENT_POSITION_U",
            Self::CardId => "MEM_CARD_ID",
            Self::MainboardVersion => "MEM_MAINBOARD_VERSION",
            Self::BedSizeX => "MEM_BED_SIZE_X",
            Self::BedSizeY => "MEM_BED_SIZE_Y",
            Self::MachineFeatures => "MEM_MACHINE_FEATURES",
        }
    }

    pub fn from_address(address: u16) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|register| register.address() == address)
    }
}

/// Memory table (``MT``), grouped by register block.
pub(crate) static MEMORY: &[(&str, u16)] = &[
    // 0x00xx — Controller and axis configuration
    ("MEM_IO_ENABLE", 0x0004),
    ("MEM_G0_VELOCITY", 0x0005),
    ("MEM_ENG_FACULA", 0x000B),
    ("MEM_HOME_VELOCITY", 0x000C),
    ("MEM_ENG_VERT_VELOCITY", 0x000E),
    ("MEM_SYSTEM_CONTROL_MODE", 0x0010),
    ("MEM_LASER_PWM_FREQUENCY_1", 0x0011),
    ("MEM_LASER_MIN_POWER_1", 0x0012),
    ("MEM_LASER_MAX_POWER_1", 0x0013),
    ("MEM_LASER_ATTENUATION", 0x0016),
    ("MEM_LASER_PWM_FREQUENCY_2", 0x0017),
    ("MEM_LASER_MIN_POWER_2", 0x0018),
    ("MEM_LASER_MAX_POWER_2", 0x0019),
    ("MEM_LASER_STANDBY_FREQUENCY_1", 0x001A),
    ("MEM_LASER_STANDBY_PULSE_1", 0x001B),
    ("MEM_LASER_STANDBY_FREQUENCY_2", 0x001C),
    ("MEM_LASER_STANDBY_PULSE_2", 0x001D),
    ("MEM_AUTO_TYPE_SPACE", 0x001E),
    ("MEM_AXIS_CONTROL_PARA_1", 0x0020),
    ("MEM_AXIS_PRECISION_1", 0x0021),
    ("MEM_PRECISION_X", 0x0021),
    ("MEM_AXIS_MAX_VELOCITY_1", 0x0023),
    ("MEM_MAX_VELOCITY_X", 0x0023),
    ("MEM_AXIS_START_VELOCITY_1", 0x0024),
    ("MEM_START_VELOCITY_X", 0x0024),
    ("MEM_AXIS_MAX_ACC_1", 0x0025),
    ("MEM_MAX_ACC_X", 0x0025),
    ("MEM_BED_SIZE_X", StatusRegister::BedSizeX as u16),
    ("MEM_AXIS_BTN_START_VEL_1", 0x0027),
    ("MEM_BTN_START_VEL_X", 0x0027),
    ("MEM_AXIS_BTN_ACC_1", 0x0028),
    ("MEM_BTN_ACC_X", 0x0028),
    ("MEM_AXIS_ESTP_ACC_1", 0x0029),
    ("MEM_ESTP_ACC_X", 0x0029),
    ("MEM_AXIS_HOME_OFFSET_1", 0x002A),
    ("MEM_HOME_OFFSET_X", 0x002A),
    ("MEM_AXIS_BACKLASH_1", 0x002B),
    ("MEM_AXIS_CONTROL_PARA_2", 0x0030),
    ("MEM_AXIS_PRECISION_2", 0x0031),
    ("MEM_PRECISION_Y", 0x0031),
    ("MEM_AXIS_MAX_VELOCITY_2", 0x0033),
    ("MEM_MAX_VELOCITY_Y", 0x0033),
    ("MEM_AXIS_START_VELOCITY_2", 0x0034),
    ("MEM_START_VELOCITY_Y", 0x0034),
    ("MEM_AXIS_MAX_ACC_2", 0x0035),
    ("MEM_MAX_ACC_Y", 0x0035),
    ("MEM_BED_SIZE_Y", StatusRegister::BedSizeY as u16),
    ("MEM_AXIS_BTN_START_VEL_2", 0x0037),
    ("MEM_BTN_START_VEL_Y", 0x0037),
    ("MEM_AXIS_BTN_ACC_2", 0x0038),
    ("MEM_BTN_ACC_Y", 0x0038),
    ("MEM_AXIS_ESTP_ACC_2", 0x0039),
    ("MEM_ESTP_ACC_Y", 0x0039),
    ("MEM_AXIS_HOME_OFFSET_2", 0x003A),
    ("MEM_HOME_OFFSET_Y", 0x003A),
    ("MEM_AXIS_BACKLASH_2", 0x003B),
    ("MEM_AXIS_CONTROL_PARA_3", 0x0040),
    ("MEM_AXIS_PRECISION_3", 0x0041),
    ("MEM_PRECISION_Z", 0x0041),
    ("MEM_AXIS_MAX_VELOCITY_3", 0x0043),
    ("MEM_MAX_VELOCITY_Z", 0x0043),
    ("MEM_AXIS_START_VELOCITY_3", 0x0044),
    ("MEM_START_VELOCITY_Z", 0x0044),
    ("MEM_AXIS_MAX_ACC_3", 0x0045),
    ("MEM_MAX_ACC_Z", 0x0045),
    ("MEM_AXIS_RANGE_3", 0x0046),
    ("MEM_TRAVEL_Z", 0x0046),
    ("MEM_AXIS_BTN_START_VEL_3", 0x0047),
    ("MEM_BTN_START_VEL_Z", 0x0047),
    ("MEM_AXIS_BTN_ACC_3", 0x0048),
    ("MEM_BTN_ACC_Z", 0x0048),
    ("MEM_AXIS_ESTP_ACC_3", 0x0049),
    ("MEM_ESTP_ACC_Z", 0x0049),
    ("MEM_AXIS_HOME_OFFSET_3", 0x004A),
    ("MEM_HOME_OFFSET_Z", 0x004A),
    ("MEM_AXIS_BACKLASH_3", 0x004B),
    ("MEM_AXIS_CONTROL_PARA_4", 0x0050),
    ("MEM_AXIS_PRECISION_4", 0x0051),
    ("MEM_PRECISION_U", 0x0051),
    ("MEM_AXIS_MAX_VELOCITY_4", 0x0053),
    ("MEM_MAX_VELOCITY_U", 0x0053),
    ("MEM_AXIS_START_VELOCITY_4", 0x0054),
    ("MEM_START_VELOCITY_U", 0x0054),
    ("MEM_AXIS_MAX_ACC_4", 0x0055),
    ("MEM_MAX_ACC_U", 0x0055),
    ("MEM_AXIS_RANGE_4", 0x0056),
    ("MEM_TRAVEL_U", 0x0056),
    ("MEM_AXIS_BTN_START_VEL_4", 0x0057),
    ("MEM_BTN_START_VEL_U", 0x0057),
    ("MEM_AXIS_BTN_ACC_4", 0x0058),
    ("MEM_BTN_ACC_U", 0x0058),
    ("MEM_AXIS_ESTP_ACC_4", 0x0059),
    ("MEM_ESTP_ACC_U", 0x0059),
    ("MEM_AXIS_HOME_OFFSET_4", 0x005A),
    ("MEM_HOME_OFFSET_U", 0x005A),
    ("MEM_AXIS_BACKLASH_4", 0x005B),
    ("MEM_MACHINE_TYPE_(0X1155,_0XAA55)", 0x0060),
    ("MEM_LASER_MIN_POWER_3", 0x0063),
    ("MEM_LASER_MAX_POWER_3", 0x0064),
    ("MEM_LASER_PWM_FREQUENCY_3", 0x0065),
    ("MEM_LASER_STANDBY_FREQUENCY_3", 0x0066),
    ("MEM_LASER_STANDBY_PULSE_3", 0x0067),
    ("MEM_LASER_MIN_POWER_4", 0x0068),
    ("MEM_LASER_MAX_POWER_4", 0x0069),
    ("MEM_LASER_PWM_FREQUENCY_4", 0x006A),
    ("MEM_LASER_STANDBY_FREQUENCY_4", 0x006B),
    ("MEM_LASER_STANDBY_PULSE_4", 0x006C),
    // 0x02xx — Motion and axis tuning
    ("MEM_SYSTEM_SETTINGS", 0x0200),
    ("MEM_TURN_VELOCITY", 0x0201),
    ("MEM_SYN_ACC", 0x0202),
    ("MEM_G0_DELAY", 0x0203),
    ("MEM_FEED_DELAY_AFTER", 0x0207),
    ("MEM_TURN_ACC", 0x0209),
    ("MEM_G0_ACC", 0x020A),
    ("MEM_FEED_DELAY_PRIOR", 0x020B),
    ("MEM_MANUAL_DIS", 0x020C),
    ("MEM_SHUT_DOWN_DELAY", 0x020D),
    ("MEM_FOCUS_DEPTH", 0x020E),
    ("MEM_GO_SCALE_BLANK", 0x020F),
    ("MEM_ARRAY_FEED_REPAY", 0x0217),
    ("MEM_ACC_RATIO", 0x021A),
    ("MEM_TURN_RATIO", 0x021B),
    ("MEM_ACC_G0_RATIO", 0x021C),
    ("MEM_ROTATE_PULSE", 0x021F),
    ("MEM_ROTATE_D", 0x0221),
    ("MEM_X_MINIMUM_ENG_VELOCITY", 0x0224),
    ("MEM_X_ENG_ACC", 0x0225),
    ("MEM_USER_PARA_1", 0x0226),
    ("MEM_Z_HOME_VELOCITY", 0x0228),
    ("MEM_Z_WORK_VELOCITY", 0x0229),
    ("MEM_Z_G0_VELOCITY", 0x022A),
    ("MEM_Z_PEN_UP_POSITION", 0x022B),
    ("MEM_U_HOME_VELOCITY", 0x022C),
    ("MEM_U_WORK_VELOCITY", 0x022D),
    ("MEM_MANUAL_FAST_SPEED", 0x0231),
    ("MEM_MANUAL_SLOW_SPEED", 0x0232),
    ("MEM_Y_MINIMUM_ENG_VELOCITY", 0x0234),
    ("MEM_Y_ENG_ACC", 0x0235),
    ("MEM_ENG_ACC_RATIO", 0x0237),
    // 0x03xx — Machine features
    ("MEM_CARD_LANGUAGE", 0x0300),
    ("MEM_PC_LOCK_1", 0x0301),
    ("MEM_PC_LOCK_2", 0x0302),
    ("MEM_PC_LOCK_3", 0x0303),
    ("MEM_PC_LOCK_4", 0x0304),
    ("MEM_PC_LOCK_5", 0x0305),
    ("MEM_PC_LOCK_6", 0x0306),
    ("MEM_PC_LOCK_7", 0x0307),
    (
        "MEM_MACHINE_FEATURES",
        StatusRegister::MachineFeatures as u16,
    ),
    ("MEM_TOTAL_LASER_WORK_TIME", 0x0311),
    // 0x04xx — Machine status and positions
    ("MEM_MACHINE_STATUS", StatusRegister::MachineStatus as u16),
    ("MEM_TOTAL_OPEN_TIME", 0x0401),
    ("MEM_TOTAL_WORK_TIME", 0x0402),
    ("MEM_TOTAL_WORK_NUMBER", 0x0403),
    ("MEM_TOTAL_DOC_NUMBER", 0x0405),
    ("MEM_PRE_WORK_TIME", 0x0408),
    ("MEM_CURRENT_POSITION_X", StatusRegister::PositionX as u16),
    ("MEM_TOTAL_WORK_LENGTH_1", 0x0423),
    ("MEM_CURRENT_POSITION_Y", StatusRegister::PositionY as u16),
    ("MEM_TOTAL_WORK_LENGTH_2", 0x0433),
    ("MEM_CURRENT_POSITION_Z", StatusRegister::PositionZ as u16),
    ("MEM_TOTAL_WORK_LENGTH_3", 0x0443),
    ("MEM_CURRENT_POSITION_U", StatusRegister::PositionU as u16),
    ("MEM_TOTAL_WORK_LENGTH_4", 0x0453),
    // 0x05xx — Identity and version
    ("MEM_CARD_ID", StatusRegister::CardId as u16),
    (
        "MEM_MAINBOARD_VERSION",
        StatusRegister::MainboardVersion as u16,
    ),
    // 0x06xx — Machine capabilities
    ("TBD:Unknown address", 0x0620),
    // 0x07xx — Extended configuration
    ("MEM_DOCUMENT_TIME", 0x0710),
    // 0x0Bxx — Machine library data
    ("MEM_CARD_LOCK", 0x0B11),
    ("MEM_UNKNOWN", 0x0B12),
];
