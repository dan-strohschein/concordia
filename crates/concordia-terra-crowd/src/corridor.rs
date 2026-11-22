//! Corridor upkeep, from DetourCrowd's `dtPathCorridor` (zlib source): merging a surface move's
//! visited polygons into the corridor's start, shortcutting toward a visible corner, and picking
//! the corners to steer by.

use concordia_core::math::Vec3;

/// Longest corridor kept, in polygons (as `nav.plan`'s path queries).
pub(crate) const MAX_CORRIDOR: usize = 8192;

/// Corners closer than this sideways to the agent are dropped (DetourCrowd's `MIN_TARGET_DIST`).
const MIN_CORNER_DISTANCE: f32 = 0.01;

/// `dtMergeCorridorStartMoved`: the corridor after moving from its first polygon through
/// `visited` (the start's polygon first). Unchanged when they share no polygon.
pub(crate) fn merge_start_moved(path: &[u64], visited: &[u64]) -> Vec<u64> {
    let Some((on_path, on_visit)) = furthest_common(path, visited) else {
        return path.to_vec();
    };
    visited[on_visit..]
        .iter()
        .rev()
        .chain(&path[on_path + 1..])
        .take(MAX_CORRIDOR)
        .copied()
        .collect()
}

/// `dtMergeCorridorStartShortcut`: the corridor after a visible shortcut through `visited` (a
/// raycast's polygons, the start's first). Unchanged when they share no polygon.
pub(crate) fn merge_start_shortcut(path: &[u64], visited: &[u64]) -> Vec<u64> {
    match furthest_common(path, visited) {
        Some((on_path, on_visit)) if on_visit > 0 => visited[..on_visit]
            .iter()
            .chain(&path[on_path..])
            .take(MAX_CORRIDOR)
            .copied()
            .collect(),
        _ => path.to_vec(),
    }
}

/// The furthest corridor polygon `visited` also holds, as (its index in `path`, in `visited`).
fn furthest_common(path: &[u64], visited: &[u64]) -> Option<(usize, usize)> {
    path.iter()
        .enumerate()
        .rev()
        .find_map(|(i, p)| visited.iter().position(|v| v == p).map(|j| (i, j)))
}

/// `findCorners`' pruning: drops the leading points within 0.01 m of `position` sideways.
pub(crate) fn corners(position: Vec3, points: &[Vec3]) -> Vec<Vec3> {
    let far = |p: &&Vec3| {
        let d = Vec3::new(p.x - position.x, 0.0, p.z - position.z);
        d.length_squared() > MIN_CORNER_DISTANCE * MIN_CORNER_DISTANCE
    };
    let first = points.iter().position(|p| far(&p)).unwrap_or(points.len());
    points[first..].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: u64 = 1;
    const B: u64 = 2;
    const C: u64 = 3;
    const D: u64 = 4;
    const E: u64 = 5;
    const X: u64 = 9;

    #[test]
    fn merge_start_moved_matches_detour() {
        // Detour finds the furthest corridor polygon the move visited, then puts the visited
        // polygons from there back to the move's end (reversed) in front of the rest.
        let path = [A, B, C, D];
        assert_eq!(merge_start_moved(&path, &[A]), vec![A, B, C, D], "stayed");
        assert_eq!(merge_start_moved(&path, &[A, B]), vec![B, C, D], "moved on");
        assert_eq!(merge_start_moved(&path, &[A, B, C]), vec![C, D]);
        assert_eq!(
            merge_start_moved(&path, &[A, X]),
            vec![X, A, B, C, D],
            "pushed off the corridor: it grows back from X"
        );
        assert_eq!(
            merge_start_moved(&path, &[X]),
            vec![A, B, C, D],
            "nothing shared"
        );
        assert_eq!(merge_start_moved(&[A], &[A, E]), vec![E, A]);
    }

    #[test]
    fn merge_start_shortcut_matches_detour() {
        // Detour finds the furthest corridor polygon the ray crossed and replaces the corridor up
        // to it with the ray's polygons before it.
        let path = [A, B, C, D];
        assert_eq!(merge_start_shortcut(&path, &[A, E, C]), vec![A, E, C, D]);
        assert_eq!(merge_start_shortcut(&path, &[A, D]), vec![A, D]);
        assert_eq!(
            merge_start_shortcut(&path, &[A]),
            vec![A, B, C, D],
            "no gain"
        );
        assert_eq!(
            merge_start_shortcut(&path, &[X, E]),
            vec![A, B, C, D],
            "nothing shared"
        );
    }

    #[test]
    fn corners_drop_points_at_the_start() {
        let at = Vec3::new(1.0, 0.2, 1.0);
        let points = [
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(1.005, 0.2, 1.005),
            Vec3::new(3.0, 0.2, 1.0),
            Vec3::new(1.0, 0.2, 1.005),
        ];
        assert_eq!(
            corners(at, &points),
            vec![points[2], points[3]],
            "only leading points within 0.01 m go"
        );
        assert_eq!(corners(at, &points[..2]), Vec::<Vec3>::new());
    }
}
