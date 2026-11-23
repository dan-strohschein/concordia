//! The `audio` debug layer: each positioned emitter's `min_distance` and `max_distance`
//! circles (green while it plays, grey idle), the listener's axes, and in the play world a line
//! from the listener to each playing positioned voice; all lifted 2 cm.

use concordia_api::{DebugGeometry, DebugLine};
use concordia_core::math::Vec3;
use concordia_scene::builtin::world_transform;
use concordia_terra::World;

use crate::components::{AudioEmitter, AudioListener};
use crate::core::AudioCore;

/// Segments per circle.
pub const CIRCLE_SEGMENTS: usize = 24;

const LIFT: Vec3 = Vec3::new(0.0, 0.02, 0.0);
const PLAYING: [f32; 4] = [0.2, 0.9, 0.35, 1.0];
const IDLE: [f32; 4] = [0.55, 0.55, 0.55, 1.0];
const VOICE: [f32; 4] = [0.95, 0.8, 0.2, 1.0];
const AXES: [[f32; 4]; 3] = [
    [0.9, 0.2, 0.2, 1.0],
    [0.2, 0.9, 0.2, 1.0],
    [0.25, 0.45, 1.0, 1.0],
];

/// The layer's lines for `world`; `voices` draws the core's playing voices (the play world).
pub fn geometry(world: &World, core: &AudioCore, voices: bool) -> DebugGeometry {
    let mut g = DebugGeometry::default();
    for id in world.entity_ids_sorted() {
        let Some(emitter) = world.get::<AudioEmitter>(id) else {
            continue;
        };
        let Some(spatial) = core.library().cue(&emitter.cue).and_then(|c| c.spatial) else {
            continue;
        };
        let Some(m) = world_transform(world, id) else {
            continue;
        };
        let centre = m.w_axis.truncate() + LIFT;
        let playing = voices && core.playing().values().any(|v| v.entity == Some(id));
        let color = if playing { PLAYING } else { IDLE };
        for radius in [spatial.min_distance, spatial.max_distance] {
            circle(&mut g, centre, radius, color);
        }
    }
    let listener = world.entity_ids_sorted().into_iter().find_map(|id| {
        world.get::<AudioListener>(id)?;
        world_transform(world, id)
    });
    if let Some(m) = listener {
        let at = m.w_axis.truncate() + LIFT;
        let (_, rotation, _) = m.to_scale_rotation_translation();
        for (axis, color) in [Vec3::X, Vec3::Y, Vec3::NEG_Z].into_iter().zip(AXES) {
            g.lines.push(DebugLine {
                from: at,
                to: at + rotation * axis,
                color,
            });
        }
        if voices {
            for position in core.playing().values().filter_map(|v| v.position) {
                g.lines.push(DebugLine {
                    from: at,
                    to: position + LIFT,
                    color: VOICE,
                });
            }
        }
    }
    g
}

fn circle(g: &mut DebugGeometry, centre: Vec3, radius: f32, color: [f32; 4]) {
    let point = |k: usize| {
        let angle = std::f32::consts::TAU * k as f32 / CIRCLE_SEGMENTS as f32;
        centre + Vec3::new(angle.cos(), 0.0, angle.sin()) * radius
    };
    for k in 0..CIRCLE_SEGMENTS {
        g.lines.push(DebugLine {
            from: point(k),
            to: point(k + 1),
            color,
        });
    }
}
