//! The local boundary, from DetourCrowd's `dtLocalBoundary` (zlib source): the nearest walls
//! around an agent, gathered from its navmesh neighbourhood.

use concordia_core::math::Vec3;
use concordia_nav::NavMesh;

use crate::avoid::distance_to_segment_sqr;
use crate::state::Wall;

/// Most walls kept (DetourCrowd's `MAX_LOCAL_SEGS`).
pub(crate) const MAX_WALLS: usize = 8;
/// Most polygons searched (DetourCrowd's `MAX_LOCAL_POLYS`).
const MAX_POLYS: usize = 16;
/// Most walls read per polygon (DetourCrowd's `DT_VERTS_PER_POLYGON × 3`).
const WALLS_PER_POLY: usize = 18;

/// `addSegment`: inserts `wall` at squared distance `d` into `kept` (nearest first, at most 8).
/// An equal distance goes before, except after the last, where it is appended while there is room.
pub(crate) fn keep_nearest(kept: &mut Vec<(f32, Wall)>, d: f32, wall: Wall) {
    match kept.last() {
        Some(&(last, _)) if d >= last => {
            if kept.len() < MAX_WALLS {
                kept.push((d, wall));
            }
        }
        _ => {
            let at = kept.iter().position(|&(k, _)| d <= k).unwrap_or(kept.len());
            kept.insert(at, (d, wall));
            kept.truncate(MAX_WALLS);
        }
    }
}

/// `dtLocalBoundary::update`: the walls within `range` of `at`, around polygon `poly`.
pub(crate) fn gather(mesh: &NavMesh, poly: u64, at: Vec3, range: f32) -> Option<Vec<Wall>> {
    let polys = mesh.local_neighbourhood(poly, at, range, MAX_POLYS).ok()?;
    let mut kept = Vec::with_capacity(MAX_WALLS);
    for p in polys {
        for [from, to] in mesh.wall_segments(p, WALLS_PER_POLY).ok()? {
            let d = distance_to_segment_sqr(at, from, to);
            if d <= range * range {
                keep_nearest(&mut kept, d, Wall { from, to });
            }
        }
    }
    Some(kept.into_iter().map(|(_, w)| w).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall(label: f32) -> Wall {
        Wall {
            from: Vec3::new(label, 0.0, 0.0),
            to: Vec3::new(label, 0.0, 1.0),
        }
    }

    fn labels(kept: &[(f32, Wall)]) -> Vec<f32> {
        kept.iter().map(|(_, w)| w.from.x).collect()
    }

    #[test]
    fn local_boundary_keeps_the_nearest_eight() {
        // Walls labelled 0..=9 at squared distances 5, 3, 3, 9, 1, 7, 2, 8, 6, 4, then one more,
        // label 10, at 8. Following `addSegment` by hand:
        // 0 → [0]; 1 (3 < 5) → [1, 0]; 2 (3 ≤ 3) goes before 1 → [2, 1, 0]; 3 (9 ≥ last) → [2, 1,
        // 0, 3]; 4 → [4, 2, 1, 0, 3]; 5 (7 ≤ 9) → [4, 2, 1, 0, 5, 3]; 6 → [4, 6, 2, 1, 0, 5, 3];
        // 7 (8 ≤ 9) → [4, 6, 2, 1, 0, 5, 7, 3], full; 8 (6 ≤ 7) → [4, 6, 2, 1, 0, 8, 5, 7], 3
        // dropped; 9 (4 ≤ 5) → [4, 6, 2, 1, 9, 0, 8, 5], 7 dropped; 10 (8 ≥ last 7, full) skipped.
        let mut kept = Vec::new();
        for (label, d) in [5.0, 3.0, 3.0, 9.0, 1.0, 7.0, 2.0, 8.0, 6.0, 4.0, 8.0]
            .into_iter()
            .enumerate()
        {
            keep_nearest(&mut kept, d, wall(label as f32));
        }
        assert_eq!(labels(&kept), vec![4.0, 6.0, 2.0, 1.0, 9.0, 0.0, 8.0, 5.0]);
        // Equal to the last while there is room: appended after it.
        let mut kept = Vec::new();
        keep_nearest(&mut kept, 2.0, wall(0.0));
        keep_nearest(&mut kept, 2.0, wall(1.0));
        keep_nearest(&mut kept, 4.0, wall(2.0));
        keep_nearest(&mut kept, 2.0, wall(3.0));
        assert_eq!(labels(&kept), vec![3.0, 0.0, 1.0, 2.0]);
    }
}
