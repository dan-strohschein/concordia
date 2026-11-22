//! The debug layers: `navmesh` (polygons filled by island and outlined) and `nav_paths` (each
//! walking agent's remaining path), lifted off the ground.

use concordia_api::{DebugGeometry, DebugLine, DebugTriangle};
use concordia_core::math::Vec3;
use concordia_scene::builtin::Transform;
use concordia_terra::World;

use crate::agent::{NavAgentState, NavStatus};
use crate::navmesh::NavMesh;

/// The color of agents' paths (linear RGBA).
const PATH: [f32; 4] = [1.0, 0.85, 0.1, 1.0];

/// How far the layer floats above the surface, in meters.
const LIFT: f32 = 0.02;

/// Island fill colors (linear RGB), repeating.
const PALETTE: [[f32; 3]; 6] = [
    [0.10, 0.55, 0.85],
    [0.90, 0.45, 0.10],
    [0.25, 0.75, 0.30],
    [0.80, 0.25, 0.60],
    [0.85, 0.80, 0.15],
    [0.20, 0.75, 0.75],
];

/// Every polygon of `meshes` as a triangle fan (by island) and an outline.
pub(crate) fn navmesh_geometry<'a>(meshes: impl Iterator<Item = &'a NavMesh>) -> DebugGeometry {
    let mut out = DebugGeometry::default();
    for mesh in meshes {
        for poly in mesh.polygons() {
            let [r, g, b] = PALETTE[poly.island as usize % PALETTE.len()];
            let fill = [r, g, b, 0.45];
            let edge = [r * 0.5, g * 0.5, b * 0.5, 1.0];
            let v: Vec<Vec3> = poly.vertices.iter().map(|p| *p + Vec3::Y * LIFT).collect();
            for i in 1..v.len().saturating_sub(1) {
                out.triangles.push(DebugTriangle {
                    points: [v[0], v[i], v[i + 1]],
                    color: fill,
                });
            }
            for i in 0..v.len() {
                out.lines.push(DebugLine {
                    from: v[i],
                    to: v[(i + 1) % v.len()],
                    color: edge,
                });
            }
        }
    }
    out
}

/// Each walking agent's remaining path: its feet to the next corner, then corner to corner.
pub(crate) fn path_geometry(world: &World) -> DebugGeometry {
    let mut out = DebugGeometry::default();
    let mut walking = Vec::new();
    for id in world.entity_ids_sorted() {
        let (Some(state), Some(t)) = (world.get::<NavAgentState>(id), world.get::<Transform>(id))
        else {
            continue;
        };
        let moving = matches!(state.status, NavStatus::Moving | NavStatus::Waiting);
        let next = state.next as usize;
        if moving && next < state.waypoints.len() {
            let mut points = vec![t.translation];
            points.extend_from_slice(&state.waypoints[next..]);
            walking.push(points);
        }
    }
    for points in walking {
        for pair in points.windows(2) {
            out.lines.push(DebugLine {
                from: pair[0] + Vec3::Y * LIFT,
                to: pair[1] + Vec3::Y * LIFT,
                color: PATH,
            });
        }
    }
    out
}
