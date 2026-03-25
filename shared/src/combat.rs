//! Shared combat logic: hitscan raycast and damage.
//!
//! Pure functions — no I/O, no allocation in the hot path.

use glam::Vec3;

use crate::physics::{PLAYER_HALF, WALLS};

/// Hitscan weapon range in world units.
pub const HITSCAN_RANGE: f32 = 100.0;

/// Damage per hitscan hit.
pub const HITSCAN_DAMAGE: u8 = 25;

/// Time in ticks before a dead player respawns.
pub const RESPAWN_TICKS: u16 = 128 * 3; // 3 seconds at 128 Hz

/// Starting / max health.
pub const MAX_HEALTH: u8 = 100;

/// Result of a hitscan ray test.
#[derive(Debug, Clone, Copy)]
pub struct HitscanResult {
    /// Did the ray hit a player?
    pub hit: bool,
    /// World position where the ray terminated (hit point or max range).
    pub end_pos: Vec3,
    /// Distance from origin to hit point.
    pub distance: f32,
}

/// Cast a ray from `origin` in `direction` (must be normalized) for up to `HITSCAN_RANGE`.
/// Tests against all players in `targets` (position, half-size) and walls.
/// Returns the closest hit (player index) or None if only walls/max-range.
///
/// `targets` is a slice of (position, entity_index) for all alive players
/// except the shooter.
pub fn hitscan(
    origin: Vec3,
    direction: Vec3,
    targets: &[(Vec3, usize)],
) -> (Option<usize>, HitscanResult) {
    let mut closest_dist = HITSCAN_RANGE;
    let mut hit_target: Option<usize> = None;

    // Check wall intersections to limit the ray
    #[allow(clippy::collapsible_if)]
    for wall in WALLS {
        if let Some(d) = ray_vs_aabb(
            origin.x,
            origin.z,
            direction.x,
            direction.z,
            wall.x_min,
            wall.z_min,
            wall.x_max,
            wall.z_max,
        ) {
            if d > 0.0 && d < closest_dist {
                closest_dist = d;
                hit_target = None;
            }
        }
    }

    // Check player intersections (simple circle test, radius = PLAYER_HALF)
    #[allow(clippy::collapsible_if)]
    for &(pos, idx) in targets {
        if let Some(d) = ray_vs_circle(
            origin.x,
            origin.z,
            direction.x,
            direction.z,
            pos.x,
            pos.z,
            PLAYER_HALF,
        ) {
            if d > 0.0 && d < closest_dist {
                closest_dist = d;
                hit_target = Some(idx);
            }
        }
    }

    let end_pos = Vec3::new(
        origin.x + direction.x * closest_dist,
        origin.y,
        origin.z + direction.z * closest_dist,
    );

    (
        hit_target,
        HitscanResult {
            hit: hit_target.is_some(),
            end_pos,
            distance: closest_dist,
        },
    )
}

/// Ray (2D, XZ plane) vs AABB. Returns distance to first intersection or None.
#[allow(clippy::too_many_arguments)]
fn ray_vs_aabb(
    ox: f32,
    oz: f32,
    dx: f32,
    dz: f32,
    x_min: f32,
    z_min: f32,
    x_max: f32,
    z_max: f32,
) -> Option<f32> {
    let (mut t_min, mut t_max) = (f32::NEG_INFINITY, f32::INFINITY);

    if dx.abs() > 1e-8 {
        let inv = 1.0 / dx;
        let t1 = (x_min - ox) * inv;
        let t2 = (x_max - ox) * inv;
        t_min = t_min.max(t1.min(t2));
        t_max = t_max.min(t1.max(t2));
    } else if ox < x_min || ox > x_max {
        return None;
    }

    if dz.abs() > 1e-8 {
        let inv = 1.0 / dz;
        let t1 = (z_min - oz) * inv;
        let t2 = (z_max - oz) * inv;
        t_min = t_min.max(t1.min(t2));
        t_max = t_max.min(t1.max(t2));
    } else if oz < z_min || oz > z_max {
        return None;
    }

    if t_min <= t_max && t_max > 0.0 {
        Some(if t_min > 0.0 { t_min } else { t_max })
    } else {
        None
    }
}

/// Ray (2D, XZ plane) vs circle. Returns distance to first intersection or None.
fn ray_vs_circle(ox: f32, oz: f32, dx: f32, dz: f32, cx: f32, cz: f32, radius: f32) -> Option<f32> {
    let fx = ox - cx;
    let fz = oz - cz;
    let a = dx * dx + dz * dz;
    let b = 2.0 * (fx * dx + fz * dz);
    let c = fx * fx + fz * fz - radius * radius;
    let disc = b * b - 4.0 * a * c;

    if disc < 0.0 {
        return None;
    }

    let sqrt_disc = disc.sqrt();
    let t = (-b - sqrt_disc) / (2.0 * a);
    if t > 0.0 {
        Some(t)
    } else {
        let t2 = (-b + sqrt_disc) / (2.0 * a);
        if t2 > 0.0 { Some(t2) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hitscan_hits_player() {
        let origin = Vec3::new(0.0, 0.0, 0.0);
        let dir = Vec3::new(1.0, 0.0, 0.0); // shoot east
        let targets = vec![(Vec3::new(5.0, 0.0, 0.0), 0)];

        let (hit, result) = hitscan(origin, dir, &targets);
        assert_eq!(hit, Some(0));
        assert!(result.hit);
        assert!(result.distance < 5.5);
    }

    #[test]
    fn hitscan_blocked_by_wall() {
        let origin = Vec3::new(0.0, 0.0, 14.0); // just north of south barrier
        let dir = Vec3::new(0.0, 0.0, 1.0); // shoot south
        // Target is beyond the south barrier (z: 12..16)
        let targets = vec![(Vec3::new(0.0, 0.0, 20.0), 0)];

        let (hit, _result) = hitscan(origin, dir, &targets);
        // Wall blocks the ray before reaching the target
        assert!(hit.is_none());
    }

    #[test]
    fn hitscan_miss() {
        let origin = Vec3::new(0.0, 0.0, 0.0);
        let dir = Vec3::new(1.0, 0.0, 0.0); // shoot east
        // Target is to the north, not east
        let targets = vec![(Vec3::new(0.0, 0.0, -10.0), 0)];

        let (hit, result) = hitscan(origin, dir, &targets);
        assert!(hit.is_none());
        assert!(!result.hit);
    }
}
