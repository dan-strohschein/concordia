//! The crowd's processors in `PrePhysics`, after `nav.plan` (DetourCrowd's `dtCrowd::update`,
//! split into steps that each write only their own agent and read others from snapshots: the
//! `CrowdGrid` of last tick's state, or a collision iteration's candidates).

use concordia_core::math::Vec3;
use concordia_nav::{CrowdAgent, NavAgent, NavAgentState, NavMeshes, PLAN};
use concordia_physics::{CharacterController, CharacterMotion};
use concordia_scene::builtin::Transform;
use concordia_terra::{Phase, Schedule, World, processor};

use crate::grid::CrowdGrid;
use crate::state::CrowdState;
use crate::steps::{avoid, collide, corridor, grid, integrate, movement, neighbours, steer};

/// `crowd.grid`: snapshots every crowd agent and character into the `CrowdGrid`.
pub const GRID: &str = "crowd.grid";
/// `crowd.corridor`: keeps corridors and picks corners (LOD-aware).
pub const CORRIDOR: &str = "crowd.corridor";
/// `crowd.neighbours`: the nearest agents and characters in range.
pub const NEIGHBOURS: &str = "crowd.neighbours";
/// `crowd.steer`: desired velocities, slowing at the end, with separation (LOD-aware).
pub const STEER: &str = "crowd.steer";
/// `crowd.avoid`: the local boundary and sampled velocity obstacles (LOD-aware).
pub const AVOID: &str = "crowd.avoid";
/// `crowd.integrate`: velocities toward the chosen one, and candidate positions.
pub const INTEGRATE: &str = "crowd.integrate";
/// `crowd.collide`: pushes overlapping agents apart (Jacobi iterations).
pub const COLLIDE: &str = "crowd.collide";
/// `crowd.move`: moves surface agents along the navmesh, drives physics agents, and arrives.
pub const MOVE: &str = "crowd.move";

/// Gives every crowd agent without one a `CrowdState` standing where it is, and the world a
/// `CrowdGrid` (at play start).
pub fn insert_crowd_states(world: &mut World) {
    let mut missing = Vec::new();
    if let Ok(mut query) = world.query::<(&CrowdAgent, &NavAgent, &Transform)>() {
        query.for_each(world, |id, (_, _, t)| missing.push((id, t.translation)));
    }
    for (id, at) in missing {
        if world.get::<CrowdState>(id).is_none() {
            let inserted = world.insert(id, standing(at));
            debug_assert!(inserted.is_ok(), "the entity was just queried");
        }
    }
    if world.resource::<CrowdGrid>().is_none() {
        world.insert_resource(CrowdGrid::default());
    }
}

/// A state standing at `at`.
pub(crate) fn standing(at: Vec3) -> CrowdState {
    CrowdState {
        surface: at,
        candidate: at,
        ..CrowdState::default()
    }
}

/// Adds the crowd's processors to `PrePhysics`, after `nav.plan`, in DetourCrowd's order.
pub fn add_crowd_processors(schedule: &mut Schedule) {
    schedule.add_processor(
        processor(GRID, Phase::PrePhysics)
            .reads::<NavAgent>()
            .reads::<CrowdAgent>()
            .reads::<CrowdState>()
            .reads::<Transform>()
            .reads::<CharacterController>()
            .reads::<CharacterMotion>()
            .writes_resource::<CrowdGrid>()
            .after(PLAN)
            .run(grid::grid_step),
    );
    schedule.add_processor(
        processor(CORRIDOR, Phase::PrePhysics)
            .lod()
            .reads::<NavAgent>()
            .reads::<CrowdAgent>()
            .reads::<Transform>()
            .writes::<NavAgentState>()
            .writes::<CrowdState>()
            .reads_resource::<NavMeshes>()
            .reads_resource::<CrowdGrid>()
            .after(GRID)
            .run(corridor::corridor_step),
    );
    schedule.add_processor(
        processor(NEIGHBOURS, Phase::PrePhysics)
            .reads::<NavAgent>()
            .reads::<CrowdAgent>()
            .writes::<CrowdState>()
            .reads_resource::<CrowdGrid>()
            .after(CORRIDOR)
            .run(neighbours::neighbours_step),
    );
    schedule.add_processor(
        processor(STEER, Phase::PrePhysics)
            .lod()
            .reads::<NavAgent>()
            .reads::<CrowdAgent>()
            .reads::<NavAgentState>()
            .writes::<CrowdState>()
            .reads_resource::<CrowdGrid>()
            .after(NEIGHBOURS)
            .run(steer::steer_step),
    );
    schedule.add_processor(
        processor(AVOID, Phase::PrePhysics)
            .lod()
            .reads::<NavAgent>()
            .reads::<CrowdAgent>()
            .reads::<NavAgentState>()
            .writes::<CrowdState>()
            .reads_resource::<NavMeshes>()
            .reads_resource::<CrowdGrid>()
            .after(STEER)
            .run(avoid::avoid_step),
    );
    schedule.add_processor(
        processor(INTEGRATE, Phase::PrePhysics)
            .reads::<CrowdAgent>()
            .reads::<NavAgentState>()
            .writes::<CrowdState>()
            .reads_resource::<CrowdGrid>()
            .after(AVOID)
            .run(integrate::integrate_step),
    );
    schedule.add_processor(
        processor(COLLIDE, Phase::PrePhysics)
            .reads::<NavAgent>()
            .reads::<CrowdAgent>()
            .writes::<CrowdState>()
            .reads_resource::<CrowdGrid>()
            .after(INTEGRATE)
            .run(collide::collide_step),
    );
    schedule.add_processor(
        processor(MOVE, Phase::PrePhysics)
            .reads::<NavAgent>()
            .reads::<CrowdAgent>()
            .reads::<CharacterController>()
            .writes::<Transform>()
            .writes::<CharacterMotion>()
            .writes::<NavAgentState>()
            .writes::<CrowdState>()
            .reads_resource::<NavMeshes>()
            .after(COLLIDE)
            .run(movement::move_step),
    );
}
