//! Shared movement and collision logic.
//!
//! Used by both the server (authoritative) and the client (prediction).
//! This module is **pure** — no I/O, no allocations in the hot path.

use glam::Vec3;

use crate::tick::TICK_RATE;
use crate::types::{InputFrame, movement};

// ── Constants ────────────────────────────────────────────────────────────────

/// Player movement speed in world-units per tick.
pub const MOVE_SPEED: f32 = 5.0 / TICK_RATE as f32;

/// Map boundary clamp (players can't walk beyond ±MAP_HALF).
pub const MAP_HALF: f32 = 95.0;

/// Mouse-look yaw sensitivity (radians per raw delta unit).
pub const YAW_SENSITIVITY: f32 = 0.003;

/// Player collision half-size (axis-aligned square).
pub const PLAYER_HALF: f32 = 0.5;

// ── Wall geometry ────────────────────────────────────────────────────────────

/// An axis-aligned rectangular wall defined by its min/max corners on the XZ plane.
#[derive(Debug, Clone, Copy)]
pub struct Wall {
    pub x_min: f32,
    pub z_min: f32,
    pub x_max: f32,
    pub z_max: f32,
}

impl Wall {
    pub const fn new(x_min: f32, z_min: f32, x_max: f32, z_max: f32) -> Self {
        Self {
            x_min,
            z_min,
            x_max,
            z_max,
        }
    }

    /// Check if a point (with a square half-extent) overlaps this wall.
    pub fn overlaps(&self, x: f32, z: f32, half: f32) -> bool {
        self.x_min < x + half
            && self.x_max > x - half
            && self.z_min < z + half
            && self.z_max > z - half
    }
}

/// Static walls placed on the map. Both server and client use this exact list.
pub const WALLS: &[Wall] = &[
    // Central L-shaped barrier
    Wall::new(-15.0, -2.0, 5.0, 2.0), // horizontal arm
    Wall::new(3.0, -15.0, 7.0, 2.0),  // vertical arm going south
    // Top-left box
    Wall::new(-40.0, -45.0, -30.0, -35.0),
    // Bottom-right box
    Wall::new(30.0, 35.0, 40.0, 45.0),
];

// ── Movement ─────────────────────────────────────────────────────────────────

/// Apply one tick of input to a player position and yaw.
///
/// This is the **single source of truth** for movement — both the server's
/// `tick()` and the client's prediction call this exact function.
pub fn apply_input(pos: &mut Vec3, yaw: &mut f32, frame: &InputFrame) {
    // Yaw rotation from mouse delta
    *yaw += frame.yaw_delta as f32 * YAW_SENSITIVITY;

    let (sin_y, cos_y) = yaw.sin_cos();
    let forward = Vec3::new(sin_y, 0.0, cos_y);
    let right = Vec3::new(cos_y, 0.0, -sin_y);

    let m = frame.movement;

    // Build desired displacement
    let mut delta = Vec3::ZERO;
    if m & movement::FORWARD != 0 {
        delta += forward;
    }
    if m & movement::BACKWARD != 0 {
        delta -= forward;
    }
    if m & movement::LEFT != 0 {
        delta -= right;
    }
    if m & movement::RIGHT != 0 {
        delta += right;
    }

    // Normalize so diagonal movement isn't faster
    if delta != Vec3::ZERO {
        delta = delta.normalize() * MOVE_SPEED;
    }

    // Try X axis independently, then Z axis — allows wall-sliding
    let new_x = pos.x + delta.x;
    if !collides_with_any(new_x, pos.z) {
        pos.x = new_x;
    }

    let new_z = pos.z + delta.z;
    if !collides_with_any(pos.x, new_z) {
        pos.z = new_z;
    }

    // Clamp to map boundary
    pos.x = pos.x.clamp(-MAP_HALF, MAP_HALF);
    pos.z = pos.z.clamp(-MAP_HALF, MAP_HALF);
}

/// Returns true if a player-sized body at (x, z) overlaps any wall.
fn collides_with_any(x: f32, z: f32) -> bool {
    for wall in WALLS {
        if wall.overlaps(x, z, PLAYER_HALF) {
            return true;
        }
    }
    false
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wall_overlap_basic() {
        let wall = Wall::new(0.0, 0.0, 10.0, 10.0);
        // Center of wall — obviously overlaps
        assert!(wall.overlaps(5.0, 5.0, 0.5));
        // Far away — no overlap
        assert!(!wall.overlaps(100.0, 100.0, 0.5));
        // Just touching the edge
        assert!(wall.overlaps(-0.4, 5.0, 0.5));
        // Just outside
        assert!(!wall.overlaps(-0.6, 5.0, 0.5));
    }

    #[test]
    fn movement_blocked_by_wall() {
        // Place player right at the left edge of the central wall
        let mut pos = Vec3::new(-16.0, 0.0, 0.0);
        // Set yaw so forward = +X
        let mut yaw: f32 = std::f32::consts::FRAC_PI_2;

        let frame = InputFrame {
            tick: 0,
            sequence: 0,
            movement: movement::FORWARD,
            yaw_delta: 0,
            pitch_delta: 0,
            flags: crate::types::PlayerFlags::new(),
        };

        // Apply many frames to push into the wall
        for _ in 0..200 {
            apply_input(&mut pos, &mut yaw, &frame);
        }

        // Player should be stopped by the wall (x_min = -15.0, player half = 0.5)
        // so player x should be <= -15.0 - 0.5 = -15.5, or close to it
        assert!(
            pos.x < -14.5,
            "Player should be blocked by wall, got x={}",
            pos.x
        );
    }

    #[test]
    fn diagonal_movement_normalized() {
        let mut pos = Vec3::ZERO;
        let mut yaw = 0.0_f32;

        // Move forward only
        let frame_fwd = InputFrame {
            tick: 0,
            sequence: 0,
            movement: movement::FORWARD,
            yaw_delta: 0,
            pitch_delta: 0,
            flags: crate::types::PlayerFlags::new(),
        };
        apply_input(&mut pos, &mut yaw, &frame_fwd);
        let fwd_dist = (pos.x * pos.x + pos.z * pos.z).sqrt();

        // Reset and move diagonally
        pos = Vec3::ZERO;
        yaw = 0.0;
        let frame_diag = InputFrame {
            tick: 0,
            sequence: 0,
            movement: movement::FORWARD | movement::RIGHT,
            yaw_delta: 0,
            pitch_delta: 0,
            flags: crate::types::PlayerFlags::new(),
        };
        apply_input(&mut pos, &mut yaw, &frame_diag);
        let diag_dist = (pos.x * pos.x + pos.z * pos.z).sqrt();

        // Diagonal distance should equal forward-only distance (normalized)
        assert!(
            (fwd_dist - diag_dist).abs() < 0.001,
            "fwd={fwd_dist} diag={diag_dist}"
        );
    }
}
