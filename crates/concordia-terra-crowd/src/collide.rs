//! Collision resolution, from DetourCrowd's loop (zlib source): penetrations pushed apart by
//! 0.7 of their depth per iteration, half each between agents, all of it against a solid.

use concordia_core::id::EntityId;
use concordia_core::math::Vec3;

use crate::steer::{coincident_push, flat};

/// Iterations per tick (DetourCrowd's).
pub(crate) const ITERATIONS: usize = 4;
/// Share of a penetration resolved per iteration (DetourCrowd's `COLLISION_RESOLVE_FACTOR`).
const RESOLVE: f32 = 0.7;

/// A contact candidate: where the other is, its radius, and whether this agent takes the whole
/// push (it can't push the other).
pub(crate) struct Contact {
    pub id: EntityId,
    pub position: Vec3,
    pub radius: f32,
    pub whole: bool,
}

/// This iteration's displacement for agent `me` at `position`, averaged over its contacts.
pub(crate) fn displacement(
    me: EntityId,
    position: Vec3,
    radius: f32,
    desired: Vec3,
    contacts: &[Contact],
) -> Vec3 {
    let mut disp = Vec3::ZERO;
    let mut count = 0.0f32;
    for c in contacts {
        let mut diff = flat(position - c.position);
        let reach = radius + c.radius;
        let d2 = diff.length_squared();
        if d2 > reach * reach {
            continue;
        }
        let d = d2.sqrt();
        let pen = if d < 0.0001 {
            diff = coincident_push(me, c.id, desired);
            0.01
        } else {
            let share = if c.whole { 1.0 } else { 0.5 };
            (reach - d) * share * RESOLVE / d
        };
        disp += diff * pen;
        count += 1.0;
    }
    match count > 0.0001 {
        true => disp / count,
        false => Vec3::ZERO,
    }
}
