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
    const SCALE: f32 = 32.0;
    const OFFSET: f32 = 1024.0; // shift so [-1024, 1024) maps to [0, 65535]

    pub fn from_vec3(v: Vec3) -> Self {
        Self {
            x: ((v.x + Self::OFFSET) * Self::SCALE) as u16,
            y: ((v.y + Self::OFFSET) * Self::SCALE) as u16,
            z: ((v.z + Self::OFFSET) * Self::SCALE) as u16,
        }
    }

    pub fn to_vec3(self) -> Vec3 {
        Vec3::new(
            f32::from(self.x) / Self::SCALE - Self::OFFSET,
            f32::from(self.y) / Self::SCALE - Self::OFFSET,
            f32::from(self.z) / Self::SCALE - Self::OFFSET,
        )
    }
}

/// Packed player flags as a single byte bitfield.
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
}

impl Default for PlayerFlags {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of a single player's state, sent from server → client each tick.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PlayerState {
    pub id: PlayerId,
    pub position: QuantizedPosition,
    pub yaw: u16,   // radians mapped to [0, 65535]
    pub pitch: i16,
    pub health: u8,
    pub flags: PlayerFlags,
}

/// WASD movement bits packed into a single byte.
pub mod movement {
    pub const FORWARD: u8 = 1 << 0;  // W
    pub const BACKWARD: u8 = 1 << 1; // S
    pub const LEFT: u8 = 1 << 2;     // A
    pub const RIGHT: u8 = 1 << 3;    // D
}

/// Input captured from the client each tick, sent to server.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct InputFrame {
    pub tick: u16,
    /// WASD bits — see `movement` constants above.
    pub movement: u8,
    pub yaw_delta: i16,
    pub pitch_delta: i16,
    pub flags: PlayerFlags,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantized_position_roundtrip() {
        let original = Vec3::new(50.5, 0.0, -30.25);
        let q = QuantizedPosition::from_vec3(original);
        let restored = q.to_vec3();
        assert!((original.x - restored.x).abs() < 1.0 / 32.0);
        assert!((original.z - restored.z).abs() < 1.0 / 32.0);
    }

    #[test]
    fn player_flags_defaults_alive() {
        let flags = PlayerFlags::new();
        assert!(flags.is_alive());
    }
}
