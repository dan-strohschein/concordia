//! Steering, from DetourCrowd's `calcSmoothSteerDirection`, `calcStraightSteerDirection`,
//! slow-down and separation (zlib source).

use concordia_core::id::EntityId;
use concordia_core::math::Vec3;

/// What steering knows about one agent.
pub(crate) struct Steering<'a> {
    pub position: Vec3,
    pub corners: &'a [Vec3],
    pub at_end: bool,
    pub radius: f32,
    pub max_speed: f32,
    pub anticipate_turns: bool,
}

/// The desired velocity toward the corners, slowed within two radii of the end.
pub(crate) fn desired(s: &Steering<'_>) -> Vec3 {
    let Some(&first) = s.corners.first() else {
        return Vec3::ZERO;
    };
    let dir = match s.anticipate_turns {
        true => smooth_direction(s.position, first, s.corners[s.corners.len().min(2) - 1]),
        false => flat(first - s.position).normalize_or_zero(),
    };
    let slow = s.radius * 2.0;
    let scale = match (s.at_end, s.corners.last()) {
        (true, Some(&end)) if slow > 0.0 => flat(end - s.position).length().min(slow) / slow,
        _ => 1.0,
    };
    dir * (s.max_speed * scale)
}

/// `calcSmoothSteerDirection`: toward the first corner, turned early toward the second.
fn smooth_direction(position: Vec3, p0: Vec3, p1: Vec3) -> Vec3 {
    let dir0 = flat(p0 - position);
    let mut dir1 = flat(p1 - position);
    let len0 = dir0.length();
    let len1 = dir1.length();
    if len1 > 0.001 {
        dir1 /= len1;
    }
    flat(dir0 - dir1 * (len0 * 0.5)).normalize_or_zero()
}

/// One neighbour as separation sees it.
pub(crate) struct Near {
    pub position: Vec3,
}

/// DetourCrowd's separation: `desired` pushed away from `neighbours` within `range`, weighted
/// 1 − (d / range)², averaged, and kept under `speed` (DetourCrowd's squared-ratio clamp).
pub(crate) fn separate(
    position: Vec3,
    desired: Vec3,
    speed: f32,
    neighbours: &[Near],
    range: f32,
    weight: f32,
) -> Vec3 {
    let mut push = Vec3::ZERO;
    let mut count = 0.0f32;
    for n in neighbours {
        let diff = flat(position - n.position);
        let d2 = diff.length_squared();
        if d2 < 0.00001 || d2 > range * range {
            continue;
        }
        let d = d2.sqrt();
        let w = weight * (1.0 - (d / range) * (d / range));
        push += diff * (w / d);
        count += 1.0;
    }
    if count <= 0.0001 {
        return desired;
    }
    let out = desired + push / count;
    let (have, want) = (out.length_squared(), speed * speed);
    match have > want {
        true => out * (want / have),
        false => out,
    }
}

/// The side DetourCrowd pushes coincident agents to: perpendicular to `desired`, mirrored by id
/// order so the two go opposite ways.
pub(crate) fn coincident_push(me: EntityId, other: EntityId, desired: Vec3) -> Vec3 {
    match me > other {
        true => Vec3::new(-desired.z, 0.0, desired.x),
        false => Vec3::new(desired.z, 0.0, -desired.x),
    }
}

/// `v` on the ground plane.
pub(crate) fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}
