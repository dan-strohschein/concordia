//! The `ai` engine module (M4 4.12b, M1, M11): registers the AI components and signals, loads
//! every brain's tree when play starts, runs `ai.think`, and serves `ai.inspect` and `ai.trace`.

use std::collections::BTreeMap;
use std::sync::Arc;

use concordia_api::{
    ApiError, ApiErrorKind, CallCtx, Engine, EngineModule, EntityRef, Method, MethodInfo,
    PlaySetup, WorldSel,
};
use concordia_reflect::{Reflect, RegistryError, TypeRegistry};
use concordia_terra::{World, register_component};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::engine::{AiCast, AiDecision, AiRun, Trees, add_ai_processors, insert_ai_resources};
use crate::{
    AiConfig, BehaviorTree, Blackboard, Brain, BtState, LlmProvider, LlmService, NodeKind,
};

/// Runtime AI for a project (`[ai]`), with an optional LLM provider the game ships (none by
/// default: `Plan` tasks then fail at once and trees fall back).
#[derive(Clone, Default)]
pub struct AiModule {
    config: AiConfig,
    provider: Option<Arc<dyn LlmProvider>>,
}

impl std::fmt::Debug for AiModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiModule")
            .field("config", &self.config)
            .field("provider", &self.provider.is_some())
            .finish()
    }
}

impl AiModule {
    /// With `config`.
    pub fn new(config: AiConfig) -> Self {
        Self {
            config,
            provider: None,
        }
    }

    /// With an LLM provider for `Plan` tasks (opt-in; a game's own choice, never a developer's
    /// subscription).
    #[must_use]
    pub fn with_provider(mut self, provider: Arc<dyn LlmProvider>) -> Self {
        self.provider = Some(provider);
        self
    }
}

impl EngineModule for AiModule {
    fn name(&self) -> &'static str {
        "ai"
    }

    fn register_types(&self, registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        register_component::<Brain>(registry)?;
        register_component::<Blackboard>(registry)?;
        register_component::<BtState>(registry)?;
        register_component::<crate::Perceiver>(registry)?;
        register_component::<crate::Faction>(registry)?;
        register_component::<crate::Memory>(registry)?;
        register_component::<crate::AiNoise>(registry)?;
        register_component::<crate::AiDamaged>(registry)?;
        register_component::<AiDecision>(registry)?;
        register_component::<AiCast>(registry)?;
        register_component::<AiRun>(registry)
    }

    fn methods(&self) -> Vec<MethodInfo> {
        vec![MethodInfo::of::<AiInspect>(), MethodInfo::of::<AiTrace>()]
    }

    /// Loads each brain's tree (one that fails is logged and its brains stay idle), gives brains
    /// their blackboard and state, and adds `ai.think`.
    fn start_play_in(&self, engine: &mut Engine, world: &mut World) -> Result<PlaySetup, ApiError> {
        let mut paths = Vec::new();
        if let Ok(mut query) = world.query::<&Brain>() {
            query.for_each(world, |id, b| paths.push((id, b.tree.clone())));
        }
        let mut trees = BTreeMap::new();
        let mut failed = BTreeMap::new();
        for (_, path) in &paths {
            if trees.contains_key(path) || failed.contains_key(path) {
                continue;
            }
            match load_tree(engine, path) {
                Ok(tree) => {
                    trees.insert(path.clone(), tree);
                }
                Err(reason) => {
                    tracing::warn!(target: "concordia::ai", tree = %path, %reason, "behavior tree not loaded");
                    failed.insert(path.clone(), reason);
                }
            }
        }
        for (id, path) in paths {
            let Some(tree) = trees.get(&path) else {
                continue;
            };
            if world.get::<Blackboard>(id).is_none() {
                let _ = world.insert(id, Blackboard::for_tree(tree));
            }
            if world.get::<BtState>(id).is_none() {
                let _ = world.insert(id, BtState::new(tree));
            }
        }
        let mut perceivers = Vec::new();
        if let Ok(mut query) = world.query::<&crate::Perceiver>() {
            query.for_each(world, |id, _| perceivers.push(id));
        }
        for id in perceivers {
            if world.get::<crate::Memory>(id).is_none() {
                let _ = world.insert(id, crate::Memory::default());
            }
        }
        world.insert_resource(Trees::with_failures(trees, failed));
        world.insert_resource(match &self.provider {
            Some(p) => LlmService::with_provider(p.clone()),
            None => LlmService::default(),
        });
        insert_ai_resources(world);
        let mut setup = PlaySetup::default();
        add_ai_processors(&mut setup.schedule, self.config);
        Ok(setup)
    }
}

/// Reads and checks a tree: a `.bt.ron`, or a Behavior `.graph.ron` compiled to one.
fn load_tree(engine: &Engine, path: &str) -> Result<BehaviorTree, String> {
    if path.ends_with(".graph.ron") {
        return load_graph_tree(engine, path);
    }
    let file = engine
        .project_file(path, ".bt.ron")
        .map_err(|e| e.message)?;
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{path}: {e}"))?;
    let tree: BehaviorTree =
        concordia_scene::read_document(&text).map_err(|e| format!("{path}: {e}"))?;
    tree.validate().map_err(|e| format!("{path}: {e}"))?;
    Ok(tree)
}

/// Reads a Behavior graph and compiles it to a tree.
fn load_graph_tree(engine: &Engine, path: &str) -> Result<BehaviorTree, String> {
    let file = engine
        .project_file(path, ".graph.ron")
        .map_err(|e| e.message)?;
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{path}: {e}"))?;
    let graph = concordia_graph::read_graph(&text).map_err(|e| format!("{path}: {e}"))?;
    crate::compile_behavior(&graph, &concordia_graph::NodeLibrary::core()).map_err(|diagnostics| {
        let lines: Vec<String> = diagnostics
            .iter()
            .map(|d| format!("{:?} at {:?}/{:?}: {}", d.code, d.node_id, d.pin, d.message))
            .collect();
        format!("{path}: {}", lines.join("; "))
    })
}

fn play(engine: &Engine) -> Result<&World, ApiError> {
    engine.world(WorldSel::play).map_err(|_| {
        ApiError::new(
            ApiErrorKind::NotPlaying,
            "brains think in play; step the sim first".to_owned(),
        )
        .suggest(["sim.step {\"ticks\": 1}".to_owned()])
    })
}

/// A node on an agent's running path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct RunningNode {
    /// Its index in the tree.
    pub node: u32,
    /// What it is (`Selector`, `MoveTo`, …).
    pub kind: String,
}

/// Show an agent's brain in play: its tree, blackboard, the running path and its clock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AiInspect;

/// `ai.inspect` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct AiInspectParams {
    /// The agent.
    pub entity: EntityRef,
}

/// `ai.inspect` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AiInspectResult {
    /// The tree's project path.
    pub tree: String,
    /// The blackboard (keys, values, version).
    pub blackboard: Value,
    /// Running nodes, root first.
    pub running: Vec<RunningNode>,
    /// The tree's clock in seconds.
    pub time: f32,
}

impl Method for AiInspect {
    const NAME: &'static str = "ai.inspect";
    const MUTATES: bool = false;
    type Params = AiInspectParams;
    type Result = AiInspectResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: AiInspectParams,
    ) -> Result<AiInspectResult, ApiError> {
        let world = play(engine)?;
        let id = p.entity.resolve(world)?;
        let brain = world.get::<Brain>(id).ok_or_else(|| {
            ApiError::new(ApiErrorKind::InvalidParams, format!("{id} has no ai.Brain"))
        })?;
        let trees = world.resource::<Trees>().cloned().unwrap_or_default();
        let tree = trees
            .get(&brain.tree)
            .map_err(|reason| ApiError::new(ApiErrorKind::InvalidParams, reason))?;
        let state = world.get::<BtState>(id).cloned().unwrap_or_default();
        let running = state
            .running()
            .into_iter()
            .map(|node| RunningNode {
                node,
                kind: kind_of(&tree.nodes[node as usize].kind),
            })
            .collect();
        let blackboard =
            serde_json::to_value(world.get::<Blackboard>(id).cloned().unwrap_or_default())
                .unwrap_or_default();
        Ok(AiInspectResult {
            tree: brain.tree.clone(),
            blackboard,
            running,
            time: state.time,
        })
    }
}

fn kind_of(kind: &NodeKind) -> String {
    let full = match kind {
        NodeKind::Task(task) => format!("{task:?}"),
        other => format!("{other:?}"),
    };
    full.split([' ', '{', '('])
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// One traced tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TracedTick {
    /// The tick.
    pub tick: u64,
    /// The agent's decisions that tick.
    pub decisions: Vec<AiDecision>,
}

/// Step play `ticks` ticks, returning each tick's decisions for one agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AiTrace;

/// `ai.trace` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct AiTraceParams {
    /// The agent.
    pub entity: EntityRef,
    /// Ticks to step (1 to 600).
    pub ticks: u32,
}

/// `ai.trace` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AiTraceResult {
    /// Per tick, in order.
    pub ticks: Vec<TracedTick>,
    /// The play world's hash after stepping.
    pub world_hash: String,
}

impl Method for AiTrace {
    const NAME: &'static str = "ai.trace";
    const MUTATES: bool = false;
    type Params = AiTraceParams;
    type Result = AiTraceResult;

    fn call(engine: &mut Engine, _: &CallCtx, p: AiTraceParams) -> Result<AiTraceResult, ApiError> {
        if !(1..=600).contains(&p.ticks) {
            return Err(ApiError::new(
                ApiErrorKind::InvalidParams,
                format!("ticks must be 1 to 600, not {}", p.ticks),
            )
            .at("/ticks"));
        }
        let id = p.entity.resolve(engine.world(WorldSel::edit)?)?;
        let mut ticks = Vec::new();
        let mut hash = String::new();
        for _ in 0..p.ticks {
            hash = engine.step_play(1)?.to_string();
            let world = play(engine)?;
            let tick = world
                .resource::<concordia_terra::SimClock>()
                .map_or(0, |c| c.tick.0);
            let decisions = world
                .pending_signals::<AiDecision>()
                .into_iter()
                .filter(|(t, _)| *t == id)
                .map(|(_, d)| d.clone())
                .collect();
            ticks.push(TracedTick { tick, decisions });
        }
        Ok(AiTraceResult {
            ticks,
            world_hash: hash,
        })
    }
}
