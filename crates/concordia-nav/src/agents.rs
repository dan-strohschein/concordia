//! The agent processors in `PrePhysics`: `nav.plan` plans paths within a per-tick budget, then
//! `nav.follow` walks each agent along its path through `CharacterMotion` and raises
//! `NavArrived` / `NavPathFailed` (2.5b.1).

use std::collections::{BTreeMap, BTreeSet};
use std::num::{NonZeroU32, NonZeroUsize};

use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_physics::{CharacterController, CharacterMotion};
use concordia_scene::builtin::Transform;
use concordia_terra::{Phase, Schedule, SystemCtx, View, Without, World, processor};

use crate::agent::{NavAgent, NavAgentState};
use crate::components::NavMeshVolume;
use crate::crowd_agent::CrowdAgent;
use crate::follow::follow;
use crate::module::NavMeshes;
use crate::plan::{Request, Volume, apply, mismatch, place, plan, volume_for, wants_plan};

/// `nav.plan`: plans paths for the agents that need one, longest-waiting first.
pub const PLAN: &str = "nav.plan";
/// `nav.follow`: walks agents along their paths and raises the signals.
pub const FOLLOW: &str = "nav.follow";

/// Agents per parallel piece in `nav.follow` (fixed, never from the thread count).
const CHUNK: NonZeroUsize = NonZeroUsize::new(256).unwrap();

/// How the agent processors run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentConfig {
    /// Most path plans per tick.
    pub plans_per_tick: NonZeroU32,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            plans_per_tick: NonZeroU32::new(32).unwrap_or(NonZeroU32::MIN),
        }
    }
}

/// Gives every `NavAgent` without one an idle `NavAgentState` (at play start).
pub fn insert_agent_states(world: &mut World) {
    let mut missing = Vec::new();
    if let Ok(mut query) = world.query::<&NavAgent>() {
        query.for_each(world, |id, _| missing.push(id));
    }
    for id in missing {
        if world.get::<NavAgentState>(id).is_none() {
            let inserted = world.insert(id, NavAgentState::default());
            debug_assert!(inserted.is_ok(), "the entity was just queried");
        }
    }
}

/// Adds `nav.plan` and then `nav.follow` to `PrePhysics`.
pub fn add_agent_processors(schedule: &mut Schedule, config: AgentConfig) {
    let mut warned: BTreeSet<(EntityId, EntityId)> = BTreeSet::new();
    schedule.add_processor(
        processor(PLAN, Phase::PrePhysics)
            .reads::<NavAgent>()
            .reads::<Transform>()
            .reads::<CharacterController>()
            .reads::<CrowdAgent>()
            .reads::<NavMeshVolume>()
            .writes::<NavAgentState>()
            .reads_resource::<NavMeshes>()
            .run(move |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                plan_all(view, ctx, config, &mut warned)
            }),
    );
    schedule.add_processor(
        processor(FOLLOW, Phase::PrePhysics)
            .reads::<NavAgent>()
            .reads::<CrowdAgent>()
            .reads::<Transform>()
            .writes::<NavAgentState>()
            .writes::<CharacterMotion>()
            .after(PLAN)
            .run(follow_all),
    );
}

/// Volumes by id.
pub(crate) fn volumes(view: &mut View<'_>) -> Vec<Volume> {
    let mut out = Vec::new();
    if let Ok(query) = view.query::<(&NavMeshVolume, &Transform)>() {
        query.for_each(|id, (v, t)| {
            out.push(Volume {
                id,
                center: t.translation,
                half: v.half_extents,
                agent_radius: v.agent_radius,
            })
        });
    }
    out.sort_by_key(|v| v.id);
    out
}

/// Each controller's (radius, height), by entity.
fn controllers(view: &mut View<'_>) -> BTreeMap<EntityId, (f32, f32)> {
    let mut out = BTreeMap::new();
    if let Ok(query) = view.query::<&CharacterController>() {
        query.for_each(|id, c| {
            out.insert(id, (c.radius, c.height));
        });
    }
    out
}

/// Each crowd agent's settings, by entity.
fn crowd_agents(view: &mut View<'_>) -> BTreeMap<EntityId, CrowdAgent> {
    let mut out = BTreeMap::new();
    if let Ok(query) = view.query::<&CrowdAgent>() {
        query.for_each(|id, c| {
            out.insert(id, c.clone());
        });
    }
    out
}

/// Gives new agents a state, marks who needs a plan, and plans the budget's worth.
fn plan_all(
    view: &mut View<'_>,
    ctx: &SystemCtx<'_>,
    config: AgentConfig,
    warned: &mut BTreeSet<(EntityId, EntityId)>,
) {
    let meshes = view.resource::<NavMeshes>().cloned().unwrap_or_default();
    let volumes = volumes(view);
    let controllers = controllers(view);
    let crowds = crowd_agents(view);
    give_states(view);
    let mut waiting: Vec<(u64, EntityId)> = Vec::new();
    if let Ok(query) = view.query::<(&NavAgent, &Transform, &mut NavAgentState)>() {
        query.for_each(|id, (agent, t, state)| {
            let crowd = crowds.contains_key(&id);
            if wants_plan(agent, state, t.translation, ctx.tick.0, crowd) {
                waiting.push((state.waiting_since, id));
            } else if crowd {
                place(agent, state, t.translation, &volumes, &meshes);
            }
        });
    }
    waiting.sort_unstable();
    let chosen: Vec<EntityId> = waiting
        .into_iter()
        .take(config.plans_per_tick.get() as usize)
        .map(|(_, id)| id)
        .collect();
    let mut failures = Vec::new();
    if let Ok(query) = view.query::<(&NavAgent, &Transform, &mut NavAgentState)>() {
        query.for_each_of(&chosen, |id, (agent, t, state)| {
            let Some(target) = agent.target else { return };
            let controller = controllers.get(&id).copied();
            let request = Request {
                entity: id,
                agent,
                target,
                position: t.translation,
                controller: controller.is_some(),
                crowd: crowds.get(&id),
            };
            let planned = plan(&request, &volumes, &meshes);
            if let (Some(c), Some((volume, _))) = (
                controller,
                volume_for(agent, t.translation, &volumes, &meshes),
            ) && warned.insert((id, volume.id))
            {
                warn_sizes(id, agent, c, volume);
            }
            if let Some(failed) = apply(state, target, planned) {
                failures.push((id, failed));
            }
        });
    }
    if let Ok(commands) = view.commands() {
        for (id, failed) in failures {
            commands.signal(id, failed);
        }
    }
}

/// Queues an idle state for each agent without one (it plans from the next tick).
fn give_states(view: &mut View<'_>) {
    let mut stated = BTreeSet::new();
    if let Ok(query) = view.query::<&NavAgentState>() {
        query.for_each(|id, _| {
            stated.insert(id);
        });
    }
    let mut missing = Vec::new();
    if let Ok(query) = view.query::<&NavAgent>() {
        query.for_each(|id, _| {
            if !stated.contains(&id) {
                missing.push(id);
            }
        });
    }
    if let Ok(commands) = view.commands() {
        for id in missing {
            commands.insert(id, NavAgentState::default());
        }
    }
}

/// Warns (once per agent and volume) when the agent's size disagrees with its body or navmesh.
fn warn_sizes(id: EntityId, agent: &NavAgent, (radius, height): (f32, f32), volume: &Volume) {
    if mismatch(agent.radius, radius) || mismatch(agent.height, height) {
        tracing::warn!(
            target: "concordia::nav", entity = %id,
            agent_radius = agent.radius, agent_height = agent.height,
            controller_radius = radius, controller_height = height,
            "NavAgent size differs from its CharacterController; the controller collides"
        );
    }
    if agent.radius > volume.agent_radius + 1e-3 {
        tracing::warn!(
            target: "concordia::nav", entity = %id, volume = %volume.id,
            agent_radius = agent.radius, navmesh_radius = volume.agent_radius,
            "NavAgent is wider than its navmesh's agent_radius; paths may brush walls"
        );
    }
}

/// Walks every agent but crowd agents one tick, in parallel; arrivals are raised as signals.
fn follow_all(view: &mut View<'_>, ctx: &SystemCtx<'_>) {
    let dt = ctx.dt;
    if let Ok(query) = view.query_filtered::<(
        &NavAgent,
        &Transform,
        &mut NavAgentState,
        &mut CharacterMotion,
    ), Without<CrowdAgent>>()
    {
        query.par_for_each_deferred(
            ctx.jobs,
            CHUNK,
            |commands, id, (agent, t, state, motion)| {
                let (velocity, arrived): (Vec3, _) = follow(agent, state, t.translation, dt);
                motion.velocity = velocity;
                if let Some(arrived) = arrived {
                    commands.signal(id, arrived);
                }
            },
        );
    }
}
