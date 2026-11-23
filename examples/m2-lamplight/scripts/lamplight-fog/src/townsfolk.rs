//! The townsfolk's pure rules: fear and the walk blend.

use concordia_core::math::Vec3;

/// How close a wisp must come to frighten a townsperson (m, on the ground).
pub const FEAR_RADIUS: f32 = 5.0;
/// Strolling speed (m/s).
pub const STROLL_SPEED: f32 = 1.2;
/// Fleeing speed (m/s).
pub const FLEE_SPEED: f32 = 3.0;
/// Seconds a frightened townsperson stays home before strolling again.
pub const CALM_SECONDS: f32 = 8.0;

/// Whether any wisp in `wisps` is within `FEAR_RADIUS` of `at` on the ground.
pub fn frightened(at: Vec3, wisps: &[Vec3]) -> bool {
    wisps.iter().any(|w| {
        let (dx, dz) = (w.x - at.x, w.z - at.z);
        dx * dx + dz * dz <= FEAR_RADIUS * FEAR_RADIUS
    })
}

/// The walk blend for moving `moved` metres in `dt` seconds (0..1 of the strolling speed).
pub fn walk_amount(moved: f32, dt: f32) -> f32 {
    if dt <= 0.0 {
        return 0.0;
    }
    (moved / dt / STROLL_SPEED).clamp(0.0, 1.0)
}
