use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Unique identifier for a connected player session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub u16);

/// Quantized 3D position using fixed-point u16 values.
/// Maps a coordinate range of [0, 2048) with 1/32 unit precision.
/// This halves bandwidth vs f32 while providing sub-unit accuracy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuantizedPosition {
    pub x: u16,
    pub y: u16,
    pub z: u16,
}

impl QuantizedPosition {
    /// Maximum world coordinate value representable.
    const SCALE: f32 = 32.0;

    pub fn from_vec3(v: Vec3) -> Self {
        Self {
            x: (v.x * Self::SCALE) as u16,
            y: (v.y * Self::SCALE) as u16,
            z: (v.z * Self::SCALE) as u16,
        }
    }

    pub fn to_vec3(self) -> Vec3 {
        Vec3::new(
            f32::from(self.x) / Self::SCALE,
            f32::from(self.y) / Self::SCALE,
            f32::from(self.z) / Self::SCALE,
        )
    }
}

/// Packed player flags as a single byte bitfield.
/// Bit 0: alive
/// Bit 1: crouching
/// Bit 2: shooting
/// Bit 3: reloading
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerFlags(pub u8);

impl PlayerFlags {
    pub const ALIVE: u8 = 1 << 0;
    pub const CROUCHING: u8 = 1 << 1;
    pub const SHOOTING: u8 = 1 << 2;
    pub const RELOADING: u8 = 1 << 3;

    pub fn new() -> Self {
        Self(Self::ALIVE)
    }

    pub fn is_alive(self) -> bool {
        self.0 & Self::ALIVE != 0
    }

    pub fn is_crouching(self) -> bool {
        self.0 & Self::CROUCHING != 0
    }

    pub fn is_shooting(self) -> bool {
        self.0 & Self::SHOOTING != 0
    }

    pub fn is_reloading(self) -> bool {
        self.0 & Self::RELOADING != 0
    }
}

impl Default for PlayerFlags {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of a single player's state, sent from server to client.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PlayerState {
    pub id: PlayerId,
    pub position: QuantizedPosition,
    pub yaw: u16,
    pub pitch: i16,
    pub health: u8,
    pub flags: PlayerFlags,
}

/// Input captured from the client each tick, sent to server.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct InputFrame {
    /// The tick number this input corresponds to.
    pub tick: u16,
    /// Movement direction packed as bitfield:
    /// Bit 0: forward, Bit 1: backward, Bit 2: left, Bit 3: right, Bit 4: jump
    pub movement: u8,
    /// Mouse yaw delta (quantized).
    pub yaw_delta: i16,
    /// Mouse pitch delta (quantized).
    pub pitch_delta: i16,
    /// Player action flags for this frame.
    pub flags: PlayerFlags,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantized_position_roundtrip() {
        let original = Vec3::new(100.5, 50.25, 200.0);
        let quantized = QuantizedPosition::from_vec3(original);
        let restored = quantized.to_vec3();
        assert!((original.x - restored.x).abs() < 1.0 / 32.0);
        assert!((original.y - restored.y).abs() < 1.0 / 32.0);
        assert!((original.z - restored.z).abs() < 1.0 / 32.0);
    }

    #[test]
    fn player_flags_defaults_alive() {
        let flags = PlayerFlags::new();
        assert!(flags.is_alive());
        assert!(!flags.is_crouching());
        assert!(!flags.is_shooting());
    }
}
