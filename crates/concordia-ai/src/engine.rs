//! Brains in the world (M4 4.12b, M3, M4, M6, M7): components, the `ai.think` processor and the
//! engine's task host. Agents change the world only through commands (nav targets, animation
//! triggers, facing) and signals, so a tick's result never depends on the thread count.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Instant;

use concordia_anim::AnimGraphPlayer;
use concordia_core::id::EntityId;
use concordia_core::math::{Quat, Vec3};
use concordia_core::rng::{SimRng, processor_seed};
use concordia_nav::{NavAgent, NavArrived, NavPathFailed};
use concordia_reflect::Reflect;
use concordia_scene::builtin::Transform;
use concordia_terra::{CommandBuffer, Phase, Schedule, SystemCtx, View, World, processor};
use serde::{Deserialize, Serialize};

use crate::{
    BbValue, BehaviorTree, Blackboard, BtState, NodeKind, Status, TaskHost, TaskKind, Ticker,
};

/// The `ai.think` processor's name.
pub const THINK: &str = "ai.think";

/// An agent: the behavior tree it runs (a project path: `ai/guard.bt.ron`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(name = "ai.Brain", category = "AI", default)]
pub struct Brain {
    /// The tree.
    pub tree: String,
}

/// Signal: an agent chose (a selector's child, a utility or random pick) or started a task (D8).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(signal, name = "ai.Decision", category = "AI", default)]
pub struct AiDecision {
    /// The node (its index in the tree).
    pub node: u32,
    /// The child chosen (0 for a task).
    pub choice: u32,
    /// `selector`, `utility`, `random` or `task`.
    pub reason: String,
    /// What the chosen node is (`MoveTo`, `Selector`, …).
    pub kind: String,
}

/// Signal: an agent casts a skill (graphs or scripts make it happen).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(signal, name = "ai.Cast", category = "AI", default)]
pub struct AiCast {
    /// The skill.
    pub skill: String,
    /// Its target, if the key held one.
    pub target: Option<EntityId>,
}

/// Signal: an agent asks a graph to run.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(signal, name = "ai.Run", category = "AI", default)]
pub struct AiRun {
    /// The graph module.
    pub graph: String,
}

/// The loaded behavior trees, by project path, and the ones that failed to load (with why).
#[derive(Debug, Clone, Default)]
pub struct Trees {
    trees: Arc<BTreeMap<String, Arc<BehaviorTree>>>,
    failed: Arc<BTreeMap<String, String>>,
}

impl Trees {
    /// From `(path, tree)` pairs.
    pub fn from_trees(trees: impl IntoIterator<Item = (String, BehaviorTree)>) -> Self {
        Self::with_failures(trees.into_iter().collect(), BTreeMap::new())
    }

    /// Loaded trees and failures by path.
    pub fn with_failures(
        trees: BTreeMap<String, BehaviorTree>,
        failed: BTreeMap<String, String>,
    ) -> Self {
        Self {
            trees: Arc::new(trees.into_iter().map(|(k, t)| (k, Arc::new(t))).collect()),
            failed: Arc::new(failed),
        }
    }

    /// The tree at `path`, or why there is none.
    pub fn get(&self, path: &str) -> Result<&Arc<BehaviorTree>, String> {
        self.trees
            .get(path)
            .ok_or_else(|| match self.failed.get(path) {
                Some(reason) => format!("{path} didn't load: {reason}"),
                None => format!("no behavior tree {path} is loaded"),
            })
    }
}

/// The last tick's thinking (not hashed: it holds wall time).
#[derive(Debug, Clone, Copy, Default)]
pub struct AiStats {
    /// Agents that re-thought.
    pub thought: u32,
    /// Microseconds `ai.think` took.
    pub micros: u64,
}

/// Where the time-slicing rotation resumes.
#[derive(Debug, Clone, Copy, Default)]
pub struct AiCursor(pub usize);

/// `[ai]` settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AiConfig {
    /// Agents that re-think a tick (the rest keep running their tasks).
    pub agents_per_tick: u32,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            agents_per_tick: 64,
        }
    }
}

/// Inserts the resources `ai.think` keeps (call once on a play world).
pub fn insert_ai_resources(world: &mut World) {
    world.insert_resource(AiStats::default());
    world.insert_resource(AiCursor::default());
    if world.resource::<Trees>().is_none() {
        world.insert_resource(Trees::default());
    }
}

/// Adds `ai.think` to `PostPhysics`.
pub fn add_ai_processors(schedule: &mut Schedule, config: AiConfig) {
    schedule.add_processor(
        processor(THINK, Phase::PostPhysics)
            .reads::<Brain>()
            .writes::<Blackboard>()
            .writes::<BtState>()
            .reads::<Transform>()
            .reads::<NavAgent>()
            .reads::<AnimGraphPlayer>()
            .receives::<NavArrived>()
            .receives::<NavPathFailed>()
            .reads_resource::<Trees>()
            .reads_resource::<SimRng>()
            .writes_resource::<AiStats>()
            .writes_resource::<AiCursor>()
            .run(move |view: &mut View<'_>, ctx: &SystemCtx<'_>| think(view, ctx, config)),
    );
}

/// What every agent's host reads (shared, read only).
struct Shared {
    transforms: BTreeMap<EntityId, Transform>,
    navs: BTreeMap<EntityId, NavAgent>,
    anims: BTreeMap<EntityId, AnimGraphPlayer>,
    arrived: BTreeSet<EntityId>,
    failed: BTreeSet<EntityId>,
    chosen: BTreeSet<EntityId>,
    trees: Trees,
    seed: u64,
    tick: u64,
    dt: f32,
}

fn think(view: &mut View<'_>, ctx: &SystemCtx<'_>, config: AiConfig) {
    let start = Instant::now();
    let Some(shared) = gather(view, ctx, config) else {
        return;
    };
    let thought = shared.chosen.len() as u32;
    if let Ok(query) = view.query::<(&Brain, &mut Blackboard, &mut BtState)>() {
        query.par_for_each_deferred(
            ctx.jobs,
            NonZeroUsize::new(32).unwrap_or(NonZeroUsize::MIN),
            |commands, id, (brain, bb, state)| {
                if shared.chosen.contains(&id) {
                    think_one(&shared, commands, id, brain, bb, state);
                }
            },
        );
    }
    if let Ok(stats) = view.resource_mut::<AiStats>() {
        *stats = AiStats {
            thought,
            micros: start.elapsed().as_micros() as u64,
        };
    }
}

/// Everything agents read this tick, and who re-thinks (the rotation's next agents, plus every
/// agent whose task heard back: a signal lasts one tick).
fn gather(view: &mut View<'_>, ctx: &SystemCtx<'_>, config: AiConfig) -> Option<Shared> {
    let trees = view.resource::<Trees>().ok()?.clone();
    let seed = view.resource::<SimRng>().ok().map_or(0, |r| {
        u64::from_le_bytes(r.seed()[..8].try_into().unwrap_or_default())
    });
    let mut transforms = BTreeMap::new();
    view.query::<&Transform>().ok()?.for_each(|id, t| {
        transforms.insert(id, *t);
    });
    let mut navs = BTreeMap::new();
    view.query::<&NavAgent>().ok()?.for_each(|id, n| {
        navs.insert(id, n.clone());
    });
    let mut anims = BTreeMap::new();
    view.query::<&AnimGraphPlayer>().ok()?.for_each(|id, a| {
        anims.insert(id, a.clone());
    });
    let arrived: BTreeSet<EntityId> = view
        .signals::<NavArrived>()
        .ok()?
        .iter()
        .map(|(id, _)| *id)
        .collect();
    let failed: BTreeSet<EntityId> = view
        .signals::<NavPathFailed>()
        .ok()?
        .iter()
        .map(|(id, _)| *id)
        .collect();
    let mut agents = Vec::new();
    view.query::<&Brain>()
        .ok()?
        .for_each(|id, _| agents.push(id));
    agents.sort();
    let cursor = view.resource::<AiCursor>().map_or(0, |c| c.0);
    let n = agents.len();
    let take = (config.agents_per_tick as usize).min(n);
    let mut chosen: BTreeSet<EntityId> =
        (0..take).map(|k| agents[(cursor + k) % n.max(1)]).collect();
    chosen.extend(
        agents
            .iter()
            .filter(|a| arrived.contains(a) || failed.contains(a)),
    );
    if let Ok(c) = view.resource_mut::<AiCursor>() {
        c.0 = if n == 0 { 0 } else { (cursor + take) % n };
    }
    Some(Shared {
        transforms,
        navs,
        anims,
        arrived,
        failed,
        chosen,
        trees,
        seed,
        tick: ctx.tick.0,
        dt: ctx.dt,
    })
}

/// One agent's tick: its tree over its blackboard, then its decisions as signals.
fn think_one(
    shared: &Shared,
    commands: &mut CommandBuffer,
    id: EntityId,
    brain: &Brain,
    bb: &mut Blackboard,
    state: &mut BtState,
) {
    let Ok(tree) = shared.trees.get(&brain.tree) else {
        return;
    };
    // A time-sliced agent catches up on the ticks it skipped.
    let skipped = match state.last_tick {
        0 => 1,
        last => shared.tick.saturating_sub(last).max(1),
    };
    state.last_tick = shared.tick;
    let mut rng = SimRng::new(processor_seed(shared.seed, shared.tick, THINK)).fork(stream(id));
    let mut decisions = Vec::new();
    let mut host = EngineHost {
        id,
        shared,
        own: shared.transforms.get(&id),
        commands,
        nav: shared.navs.get(&id).cloned(),
    };
    Ticker {
        tree,
        state,
        bb,
        host: &mut host,
        rng: &mut rng,
        dt: shared.dt * skipped as f32,
        decisions: &mut decisions,
    }
    .tick();
    for d in decisions {
        let kind = kind_name(&tree.nodes[d.choice_node(tree) as usize].kind);
        host.commands.signal(
            id,
            AiDecision {
                node: d.node,
                choice: d.choice,
                reason: d.reason,
                kind,
            },
        );
    }
}

/// A per-entity random stream.
fn stream(id: EntityId) -> u64 {
    let uuid = id.as_uuid();
    u64::from_le_bytes(uuid.as_bytes()[8..].try_into().unwrap_or_default())
}

/// A node kind's short name (`Selector`, `MoveTo`, …).
fn kind_name(kind: &NodeKind) -> String {
    let full = match kind {
        NodeKind::Task(task) => format!("{task:?}"),
        other => format!("{other:?}"),
    };
    full.split([' ', '{', '('])
        .next()
        .unwrap_or_default()
        .to_owned()
}

impl crate::Decision {
    /// The node the decision picked: the chosen child of a selector, or the task itself.
    fn choice_node(&self, tree: &BehaviorTree) -> u32 {
        match self.reason.as_str() {
            "task" => self.node,
            _ => tree
                .children(self.node)
                .nth(self.choice as usize)
                .unwrap_or(self.node),
        }
    }
}

/// Runs a tree's engine tasks for one agent through commands and signals.
struct EngineHost<'a> {
    id: EntityId,
    shared: &'a Shared,
    own: Option<&'a Transform>,
    commands: &'a mut CommandBuffer,
    nav: Option<NavAgent>,
}

impl EngineHost<'_> {
    /// The point a key names: a `Vec3`, or an entity's position.
    fn point(&self, bb: &Blackboard, key: &str) -> Option<Vec3> {
        match bb.get(key)? {
            BbValue::Vec3(v) => Some(Vec3::from_array(*v)),
            BbValue::Entity(e) => self.shared.transforms.get(e).map(|t| t.translation),
            _ => None,
        }
    }

    fn set_nav_target(&mut self, target: Option<Vec3>) -> bool {
        let Some(nav) = self.nav.as_mut() else {
            return false;
        };
        nav.target = target;
        let nav = nav.clone();
        self.commands.insert(self.id, nav);
        true
    }
}

impl TaskHost for EngineHost<'_> {
    fn start(&mut self, _: u32, task: &TaskKind, bb: &mut Blackboard) -> Status {
        match task {
            TaskKind::MoveTo { key } => match self.point(bb, key) {
                Some(p) if self.set_nav_target(Some(p)) => Status::Running,
                _ => Status::Failure,
            },
            TaskKind::PlayAnim { trigger } => match self.shared.anims.get(&self.id) {
                Some(anim) => {
                    let mut anim = anim.clone();
                    anim.triggers.push(trigger.clone());
                    self.commands.insert(self.id, anim);
                    Status::Success
                }
                None => Status::Failure,
            },
            TaskKind::FaceTarget { key } => match (self.point(bb, key), self.own) {
                (Some(p), Some(own)) => {
                    let d = p - own.translation;
                    if d.x.abs() + d.z.abs() > 1e-6 {
                        let mut t = *own;
                        t.rotation = Quat::from_rotation_y(d.x.atan2(d.z));
                        self.commands.insert(self.id, t);
                    }
                    Status::Success
                }
                _ => Status::Failure,
            },
            TaskKind::CastSkill { skill, target_key } => {
                let target = match bb.get(target_key) {
                    Some(BbValue::Entity(e)) => Some(*e),
                    _ => None,
                };
                self.commands.signal(
                    self.id,
                    AiCast {
                        skill: skill.clone(),
                        target,
                    },
                );
                Status::Success
            }
            TaskKind::RunGraph { graph } => {
                self.commands.signal(
                    self.id,
                    AiRun {
                        graph: graph.clone(),
                    },
                );
                Status::Success
            }
            // The planner (4.12d) and host-defined tasks have no engine form here.
            _ => Status::Failure,
        }
    }

    fn update(&mut self, _: u32, task: &TaskKind, _: &mut Blackboard) -> Status {
        match task {
            TaskKind::MoveTo { .. } if self.shared.failed.contains(&self.id) => Status::Failure,
            TaskKind::MoveTo { .. } if self.shared.arrived.contains(&self.id) => Status::Success,
            TaskKind::MoveTo { .. } => Status::Running,
            _ => Status::Failure,
        }
    }

    fn abort(&mut self, _: u32, task: &TaskKind) {
        if let TaskKind::MoveTo { .. } = task {
            self.set_nav_target(None);
        }
    }
}
