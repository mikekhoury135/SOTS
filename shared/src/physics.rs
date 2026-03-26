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

/// Wall height in world units (floor to ceiling).
pub const WALL_HEIGHT: f32 = 3.0;

/// Camera / eye height above the ground plane.
pub const EYE_HEIGHT: f32 = 1.5;

/// Player render height (full body).
pub const PLAYER_HEIGHT: f32 = 2.0;

/// Ceiling height — visual ceiling plane, also used for collision.
pub const CEILING_HEIGHT: f32 = 4.0;

/// Gravitational acceleration (world-units / second²).
pub const GRAVITY: f32 = 22.0;

/// Vertical velocity applied on jump (world-units / second).
pub const JUMP_VEL: f32 = 8.5;

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
///
/// All walls are offset from spawn (Vec3::ZERO) so players can move freely
/// from the start and walk a short distance to reach and test wall collision.
pub const WALLS: &[Wall] = &[
    // North barrier — walk forward (W, -Z) ~12 units from spawn
    Wall::new(-8.0, -16.0, 8.0, -12.0),
    // East pillar — strafe right (D) ~15 units from spawn
    Wall::new(15.0, -6.0, 21.0, 6.0),
    // South-west box — further exploration
    Wall::new(-40.0, 22.0, -28.0, 35.0),
    // North-east wall — far corner
    Wall::new(28.0, -34.0, 44.0, -30.0),
];

// ── Movement ─────────────────────────────────────────────────────────────────

/// Apply one tick of input to a player position, yaw, and vertical velocity.
///
/// This is the **single source of truth** for movement — both the server's
/// `tick()` and the client's prediction call this exact function.
pub fn apply_input(pos: &mut Vec3, yaw: &mut f32, vy: &mut f32, frame: &InputFrame) {
    let dt = 1.0 / TICK_RATE as f32;

    // ── Yaw rotation ──────────────────────────────────────────────────────────
    *yaw += frame.yaw_delta as f32 * YAW_SENSITIVITY;

    let (sin_y, cos_y) = yaw.sin_cos();
    // Screen-up is -Z (camera up vector = Vec3::NEG_Z), so forward faces -Z at yaw=0.
    let forward = Vec3::new(sin_y, 0.0, -cos_y);
    let right = Vec3::new(cos_y, 0.0, sin_y);

    let m = frame.movement;

    // ── Horizontal movement ───────────────────────────────────────────────────
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

    // ── Vertical movement (gravity + jump) ────────────────────────────────────
    let grounded = pos.y <= 0.0;

    if m & movement::JUMP != 0 && grounded {
        *vy = JUMP_VEL;
    }

    // Apply gravity every tick
    *vy -= GRAVITY * dt;

    pos.y += *vy * dt;

    // Land on the floor
    if pos.y <= 0.0 {
        pos.y = 0.0;
        *vy = 0.0_f32.max(*vy); // absorb downward velocity, keep upward if any
    }

    // Bump head on ceiling
    let head_top = pos.y + PLAYER_HEIGHT;
    if head_top >= CEILING_HEIGHT {
        pos.y = CEILING_HEIGHT - PLAYER_HEIGHT;
        if *vy > 0.0 {
            *vy = 0.0;
        }
    }
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
        // Place player just west of the east pillar (x_min=15, z: -6..6).
        // Walk east (yaw = PI/2 → forward = +X) into the pillar.
        let mut pos = Vec3::new(13.0, 0.0, 0.0);
        let mut yaw: f32 = std::f32::consts::FRAC_PI_2; // forward = +X

        let frame = InputFrame {
            tick: 0,
            sequence: 0,
            movement: movement::FORWARD,
            yaw_delta: 0,
            pitch_delta: 0,
            flags: crate::types::PlayerFlags::new(),
        };

        // Apply many frames — player should be stopped by the east pillar (x_min=15)
        for _ in 0..200 {
            apply_input(&mut pos, &mut yaw, &mut 0.0_f32, &frame);
        }

        // Stopped at x = 15.0 - PLAYER_HALF (0.5) = 14.5
        assert!(
            pos.x < 15.0,
            "Player should be blocked by east pillar, got x={}",
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
        apply_input(&mut pos, &mut yaw, &mut 0.0_f32, &frame_fwd);
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
        apply_input(&mut pos, &mut yaw, &mut 0.0_f32, &frame_diag);
        let diag_dist = (pos.x * pos.x + pos.z * pos.z).sqrt();

        // Diagonal distance should equal forward-only distance (normalized)
        assert!(
            (fwd_dist - diag_dist).abs() < 0.001,
            "fwd={fwd_dist} diag={diag_dist}"
        );
    }
}
