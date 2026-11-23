//! The `crowd` debug layer: each crowd agent's ring, velocities, corners, walls and neighbour
//! links.

use std::collections::BTreeSet;

use concordia_api::{DebugGeometry, DebugLine};
use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_nav::{CIRCLE_STEPS, CrowdAgent, NavAgent, circle_step};
use concordia_scene::builtin::Transform;
use concordia_terra::World;

use crate::state::CrowdState;

/// Lines sit this far above the feet, as nav's layers.
const LIFT: f32 = 0.02;
/// Velocities are drawn as this many seconds of travel.
const SECONDS: f32 = 0.5;
const RING: [f32; 4] = [0.3, 0.8, 1.0, 1.0];
const VELOCITY: [f32; 4] = [0.2, 1.0, 0.3, 1.0];
const DESIRED: [f32; 4] = [1.0, 0.5, 0.1, 1.0];
const CORNERS: [f32; 4] = [1.0, 0.85, 0.1, 1.0];
const LINK: [f32; 4] = [0.8, 0.3, 1.0, 1.0];
const WALL: [f32; 4] = [1.0, 0.2, 0.2, 1.0];

/// Every crowd agent's geometry, by id.
pub(crate) fn crowd_geometry(world: &World) -> DebugGeometry {
    let mut out = DebugGeometry::default();
    let mut drawn: BTreeSet<(EntityId, EntityId)> = BTreeSet::new();
    for id in world.entity_ids_sorted() {
        let (Some(agent), Some(state), Some(t)) = (
            world.get::<NavAgent>(id),
            world.get::<CrowdState>(id),
            world.get::<Transform>(id),
        ) else {
            continue;
        };
        if world.get::<CrowdAgent>(id).is_none() {
            continue;
        }
        let feet = t.translation;
        ring(&mut out, feet, agent.radius);
        arrow(&mut out, feet, state.velocity, VELOCITY);
        arrow(&mut out, feet, state.desired_velocity, DESIRED);
        let mut from = feet;
        for &corner in &state.corners {
            line(&mut out, from, corner, CORNERS);
            from = corner;
        }
        for w in &state.walls {
            line(&mut out, w.from, w.to, WALL);
        }
        for &other in &state.neighbours {
            let pair = (id.min(other), id.max(other));
            if let Some(o) = world.get::<Transform>(other)
                && drawn.insert(pair)
            {
                line(&mut out, feet, o.translation, LINK);
            }
        }
    }
    out
}

fn ring(out: &mut DebugGeometry, at: Vec3, radius: f32) {
    let point = |k: usize| {
        let (c, s) = circle_step(k);
        at + Vec3::new(c * radius, 0.0, s * radius)
    };
    for k in 0..CIRCLE_STEPS {
        line(out, point(k), point(k + 1), RING);
    }
}

fn arrow(out: &mut DebugGeometry, at: Vec3, velocity: Vec3, color: [f32; 4]) {
    if velocity.length_squared() > 0.0 {
        line(out, at, at + velocity * SECONDS, color);
    }
}

fn line(out: &mut DebugGeometry, from: Vec3, to: Vec3, color: [f32; 4]) {
    out.lines.push(DebugLine {
        from: from + Vec3::Y * LIFT,
        to: to + Vec3::Y * LIFT,
        color,
    });
}
