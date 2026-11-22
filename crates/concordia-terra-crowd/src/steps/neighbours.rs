//! `crowd.neighbours`: the nearest agents and characters in range.

use concordia_core::id::EntityId;
use concordia_nav::CrowdAgent;
use concordia_terra::{SystemCtx, View};

use super::*;
use crate::grid::{CrowdGrid, GridEntry};
use crate::state::{CrowdState, MAX_NEIGHBOURS};
use crate::steer::flat;

/// `crowd.neighbours`: everyone in range, overlapping in height, nearest first (ties by id).
pub(crate) fn neighbours_step(view: &mut View<'_>, ctx: &SystemCtx<'_>) {
    let grid = view.resource::<CrowdGrid>().cloned().unwrap_or_default();
    let Ok(query) = view.query::<(&CrowdAgent, &mut CrowdState)>() else {
        return;
    };
    query.par_for_each(ctx.jobs, CHUNK, |id, (tuning, state)| {
        state.neighbours = match grid.get(id) {
            Some(me) => nearest(&grid, me, tuning.collision_query_range),
            None => Vec::new(),
        };
    });
}

/// DetourCrowd's `getNeighbours`, taking everything in range rather than the grid's first 32.
pub(crate) fn nearest(grid: &CrowdGrid, me: &GridEntry, range: f32) -> Vec<EntityId> {
    let mut found: Vec<(f32, EntityId)> = grid
        .around(me.position, range)
        .filter(|o| o.id != me.id)
        .filter(|o| (me.position.y - o.position.y).abs() < (me.height + o.height) * 0.5)
        .map(|o| (flat(me.position - o.position).length_squared(), o.id))
        .filter(|(d2, _)| *d2 <= range * range)
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    found.truncate(MAX_NEIGHBOURS);
    found.into_iter().map(|(_, id)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u64) -> EntityId {
        format!("018bcfe5-6800-7000-a424-{n:012x}").parse().unwrap()
    }

    fn entry(n: u64, x: f32, y: f32) -> GridEntry {
        GridEntry {
            id: id(n),
            position: Vec3::new(x, y, 0.0),
            radius: 0.25,
            height: 1.8,
            velocity: Vec3::ZERO,
            desired_velocity: Vec3::ZERO,
            solid: false,
            crowd: true,
        }
    }

    #[test]
    fn neighbours_are_the_nearest_six_by_distance_then_id() {
        // Ten others around agent 1 at x = 0: 9 and 8 at 0.5 m (a tie, so 8 first), then 0.8, 1.0,
        // 1.2, 1.5 and 2.0 m; 7 at 3.0 m is seventh; 20 is out of the 3.6 m range; 21 is 1 m
        // away but 1.9 m up (not overlapping: |Δy| ≥ 1.8). The nearest six, by (distance², id).
        let me = entry(1, 0.0, 0.0);
        let others = [
            entry(2, 2.0, 0.0),
            entry(3, -1.5, 0.0),
            entry(4, 1.2, 0.0),
            entry(5, -1.0, 0.0),
            entry(6, 0.8, 0.0),
            entry(7, 3.0, 0.0),
            entry(8, -0.5, 0.0),
            entry(9, 0.5, 0.0),
            entry(20, 3.7, 0.0),
            entry(21, 1.0, 1.9),
        ];
        let grid = CrowdGrid::new([&[me][..], &others].concat(), 3.6);
        let want: Vec<EntityId> = [8, 9, 6, 5, 4, 3].into_iter().map(id).collect();
        assert_eq!(nearest(&grid, &me, 3.6), want);
    }
}
