//! Decoding of Ruida ``GET_SETTING`` replies into machine status.

use crate::core::types::DeviceStatus;
use crate::ruida::codec::{self, Reply};
use crate::ruida::protocol::{
    MachineFeatures, StatusRegister, MACHINE_STATUS_MOVING,
    MACHINE_STATUS_PAUSED, MACHINE_STATUS_RUNNING,
};

/// The machine state derived from the controller's status replies.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MachineStatus {
    pub running: bool,
    pub paused: bool,
    pub moving: bool,
    /// X, Y, Z and U positions in millimetres.
    pub position: [Option<f64>; 4],
    pub card_id: Option<u32>,
    /// Bed size in millimetres.
    pub bed_size: [Option<f64>; 2],
    pub mainboard_version: Option<String>,
    pub features: Option<u32>,
}

impl MachineStatus {
    /// Applies one reply. Returns whether anything changed.
    pub fn apply(&mut self, reply: &Reply) -> bool {
        let before = self.clone();
        let Some(register) = StatusRegister::from_address(reply.address) else {
            return false;
        };
        match register {
            StatusRegister::MachineStatus => {
                let value = codec::to_uint(&reply.data) as u32;
                self.running = value & MACHINE_STATUS_RUNNING != 0;
                self.paused = value & MACHINE_STATUS_PAUSED != 0;
                self.moving = value & MACHINE_STATUS_MOVING != 0;
            }
            StatusRegister::PositionX => {
                self.position[0] = decode_position(&reply.data)
            }
            StatusRegister::PositionY => {
                self.position[1] = decode_position(&reply.data)
            }
            StatusRegister::PositionZ => {
                self.position[2] = decode_position(&reply.data)
            }
            StatusRegister::PositionU => {
                self.position[3] = decode_position(&reply.data)
            }
            StatusRegister::CardId => {
                self.card_id = Some(codec::to_uint(&reply.data) as u32)
            }
            StatusRegister::MainboardVersion => {
                self.mainboard_version = Some(decode_string(&reply.data))
            }
            StatusRegister::BedSizeX => {
                self.bed_size[0] = decode_position(&reply.data)
            }
            StatusRegister::BedSizeY => {
                self.bed_size[1] = decode_position(&reply.data)
            }
            StatusRegister::MachineFeatures => {
                self.features = Some(codec::to_uint(&reply.data) as u32)
            }
        }
        *self != before
    }

    /// Decoded machine feature flags, when the controller reported them.
    pub fn machine_features(&self) -> Option<MachineFeatures> {
        self.features.map(MachineFeatures::from_bits)
    }

    /// The device status the UI should show.
    pub fn device_status(&self) -> DeviceStatus {
        if self.paused {
            DeviceStatus::Hold
        } else if self.running {
            DeviceStatus::Run
        } else {
            DeviceStatus::Idle
        }
    }
}

fn decode_position(data: &[u8]) -> Option<f64> {
    (!data.is_empty()).then(|| codec::to_int(data) as f64 / 1000.0)
}

fn decode_string(data: &[u8]) -> String {
    let end = data
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).into_owned()
}
