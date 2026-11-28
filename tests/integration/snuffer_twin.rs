//! The Snuffer's hand-written Rust twin (M4 4.13b, N4): the same choices as `ai/snuffer.graph.ron`,
//! written as a state machine instead of a tree. Run over the same inputs, the graph brain and the
//! twin must leave the world identical on every tick, except for each brain's own bookkeeping.
//!
//! The twin follows the behavior-tree runtime's timing (tick.rs): tree time grows by `dt` before
//! each tick; a finished tree starts again next tick; a `Lower` condition interrupts the lower
//! branch when it starts to hold, never its own; a MoveTo already within arrival distance succeeds
//! at once, and otherwise sets the nav target and waits for `NavArrived` or `NavPathFailed`.

use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_ai::{AiCast, BbValue, Blackboard};
use concordia_api::{ApiError, EngineModule, PlaySetup};
use concordia_core::id::EntityId;
use concordia_core::math::{Quat, Vec3};
use concordia_nav::{NavAgent, NavArrived, NavPathFailed};
use concordia_reflect::{Reflect, RegistryError, TypeRegistry};
use concordia_scene::builtin::Transform;
use concordia_terra::{CommandBuffer, Phase, Schedule, SystemCtx, View, World, processor};
use serde::{Deserialize, Serialize};

const EPS: f32 = 1e-6;
const POST: [f32; 3] = [0.0, 0.0, 26.0];
const COOLDOWN: f32 = 2.0;
const HUNT_WAIT: f32 = 0.25;
const POST_WAIT: f32 = 3.0;

/// Where the twin is in its (unwritten) tree.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Step {
    /// Nothing running: the tree starts from the top.
    #[default]
    Idle,
    /// Hunting: walking to where the lamplighter was.
    HuntMove,
    /// Hunting: cooling down.
    HuntWait,
    /// Going back: walking to the post.
    ReturnMove,
    /// At the post: waiting.
    ReturnWait,
}

/// The twin's state (its bookkeeping, like a brain's `ai.BtState`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "twin.SnufferTwin", default)]
pub struct SnufferTwin {
    /// What runs.
    pub step: Step,
    /// Tree time (s).
    pub time: f32,
    /// When the running wait started.
    pub start: f32,
    /// The cast's cooldown ends.
    pub until: f32,
}

/// The twin as an engine module: entities with `twin.SnufferTwin` get the graph's blackboard and
/// are thought for by `twin.think` in `PostPhysics`, where `ai.think` runs.
#[derive(Debug, Default)]
pub struct TwinModule;

impl EngineModule for TwinModule {
    fn name(&self) -> &'static str {
        "twin"
    }

    fn register_types(&self, registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        concordia_terra::register_component::<SnufferTwin>(registry)
    }

    fn start_play(&self, world: &mut World) -> Result<PlaySetup, ApiError> {
        let graph = concordia_graph::read_graph(
            &std::fs::read_to_string(
                concordia_tests::repo_root().join("examples/m2-lamplight/ai/snuffer.graph.ron"),
            )
            .expect("the Snuffer's graph"),
        )
        .expect("it parses");
        let tree = concordia_ai::compile_behavior(&graph, &concordia_graph::NodeLibrary::core())
            .expect("it compiles");
        let mut twins = Vec::new();
        if let Ok(mut q) = world.query::<&SnufferTwin>() {
            q.for_each(world, |id, _| twins.push(id));
        }
        for id in twins {
            let _ = world.insert(id, Blackboard::for_tree(&tree));
        }
        let mut setup = PlaySetup::default();
        add_twin(&mut setup.schedule);
        Ok(setup)
    }
}

fn add_twin(schedule: &mut Schedule) {
    schedule.add_processor(
        processor("twin.think", Phase::PostPhysics)
            .after("ai.sense")
            .reads::<SnufferTwin>()
            .writes::<Blackboard>()
            .writes::<SnufferTwin>()
            .reads::<Transform>()
            .reads::<NavAgent>()
            .receives::<NavArrived>()
            .receives::<NavPathFailed>()
            .run(think),
    );
}

/// What a twin reads this tick (as of the processor's start, like `ai.think`).
struct Seen {
    transforms: std::collections::BTreeMap<EntityId, Transform>,
    navs: std::collections::BTreeMap<EntityId, NavAgent>,
    arrived: BTreeSet<EntityId>,
    failed: BTreeSet<EntityId>,
    dt: f32,
}

fn think(view: &mut View<'_>, ctx: &SystemCtx<'_>) {
    let mut seen = Seen {
        transforms: Default::default(),
        navs: Default::default(),
        arrived: Default::default(),
        failed: Default::default(),
        dt: ctx.dt,
    };
    if let Ok(q) = view.query::<&Transform>() {
        q.for_each(|id, t| {
            seen.transforms.insert(id, *t);
        });
    }
    if let Ok(q) = view.query::<&NavAgent>() {
        q.for_each(|id, n| {
            seen.navs.insert(id, n.clone());
        });
    }
    if let Ok(s) = view.signals::<NavArrived>() {
        seen.arrived = s.iter().map(|(id, _)| *id).collect();
    }
    if let Ok(s) = view.signals::<NavPathFailed>() {
        seen.failed = s.iter().map(|(id, _)| *id).collect();
    }
    if let Ok(q) = view.query::<(&mut SnufferTwin, &mut Blackboard)>() {
        q.par_for_each_deferred(ctx.jobs, NonZeroUsize::MIN, |commands, id, (twin, bb)| {
            Twin {
                seen: &seen,
                commands,
                id,
                nav: seen.navs.get(&id).cloned(),
                bb,
            }
            .tick(twin);
        });
    }
}

/// One twin's tick.
struct Twin<'a> {
    seen: &'a Seen,
    commands: &'a mut CommandBuffer,
    id: EntityId,
    nav: Option<NavAgent>,
    bb: &'a mut Blackboard,
}

impl Twin<'_> {
    fn tick(&mut self, s: &mut SnufferTwin) {
        s.time += self.seen.dt;
        if matches!(s.step, Step::ReturnMove | Step::ReturnWait) && self.knows_target() {
            if s.step == Step::ReturnMove {
                self.set_nav(None);
            }
            s.step = Step::Idle;
        }
        let hunt = match s.step {
            Step::Idle if self.knows_target() => self.hunt_fresh(s),
            Step::Idle => false,
            Step::HuntMove => self.hunt_moving(s),
            Step::HuntWait => {
                if s.time - s.start + EPS >= HUNT_WAIT {
                    s.step = Step::Idle;
                }
                return;
            }
            Step::ReturnMove => return self.return_moving(s),
            Step::ReturnWait => {
                if s.time - s.start + EPS >= POST_WAIT {
                    s.step = Step::Idle;
                }
                return;
            }
        };
        // The hunt failed (or never held): the selector tries going back, this tick.
        if !hunt && s.step == Step::Idle {
            self.return_fresh(s);
        }
    }

    /// `target` is set (the condition).
    fn knows_target(&self) -> bool {
        self.bb.get("target").is_some_and(|v| *v != BbValue::None)
    }

    fn point(&self, key: &str) -> Option<Vec3> {
        match self.bb.get(key)? {
            BbValue::Vec3(v) => Some(Vec3::from_array(*v)),
            BbValue::Entity(e) => self.seen.transforms.get(e).map(|t| t.translation),
            _ => None,
        }
    }

    fn own(&self) -> Option<&Transform> {
        self.seen.transforms.get(&self.id)
    }

    fn set_nav(&mut self, target: Option<Vec3>) -> bool {
        let Some(nav) = self.nav.as_mut() else {
            return false;
        };
        nav.target = target;
        let nav = nav.clone();
        self.commands.insert(self.id, nav);
        true
    }

    /// Starts a MoveTo: `Some(true)` arrived at once, `Some(false)` walking, `None` failed.
    fn move_to(&mut self, p: Option<Vec3>) -> Option<bool> {
        let p = p?;
        let here = match (self.nav.as_ref(), self.own()) {
            (Some(nav), Some(own)) => nav.near(own.translation, p, nav.target_desired_distance),
            _ => false,
        };
        match here {
            true => Some(true),
            false => self.set_nav(Some(p)).then_some(false),
        }
    }

    /// A walk heard back: `Some(true)` arrived, `Some(false)` still walking, `None` failed.
    fn walked(&self) -> Option<bool> {
        match (
            self.seen.failed.contains(&self.id),
            self.seen.arrived.contains(&self.id),
        ) {
            (true, _) => None,
            (_, arrived) => Some(arrived),
        }
    }

    /// The hunt from its start; false when it failed.
    fn hunt_fresh(&mut self, s: &mut SnufferTwin) -> bool {
        match self.move_to(self.point("target_position")) {
            None => false,
            Some(false) => {
                s.step = Step::HuntMove;
                true
            }
            Some(true) => self.face_and_cast(s),
        }
    }

    fn hunt_moving(&mut self, s: &mut SnufferTwin) -> bool {
        match self.walked() {
            None => {
                s.step = Step::Idle;
                false
            }
            Some(false) => true,
            Some(true) => self.face_and_cast(s),
        }
    }

    /// Face the lamplighter, then cast (or wait out the cooldown); false when facing failed.
    fn face_and_cast(&mut self, s: &mut SnufferTwin) -> bool {
        let (Some(p), Some(own)) = (self.point("target"), self.own().copied()) else {
            s.step = Step::Idle;
            return false;
        };
        let d = p - own.translation;
        if d.x.abs() + d.z.abs() > 1e-6 {
            let mut t = own;
            t.rotation = Quat::from_rotation_y(d.x.atan2(d.z));
            self.commands.insert(self.id, t);
        }
        if s.time + EPS < s.until {
            s.start = s.time;
            s.step = Step::HuntWait;
            return true;
        }
        let target = match self.bb.get("target") {
            Some(BbValue::Entity(e)) => Some(*e),
            _ => None,
        };
        self.commands.signal(
            self.id,
            AiCast {
                skill: "snuff".into(),
                target,
            },
        );
        s.until = s.time + COOLDOWN;
        s.step = Step::Idle;
        true
    }

    fn return_fresh(&mut self, s: &mut SnufferTwin) {
        let _ = self.bb.set("home", BbValue::Vec3(POST));
        match self.move_to(self.point("home")) {
            None => s.step = Step::Idle,
            Some(false) => s.step = Step::ReturnMove,
            Some(true) => {
                s.start = s.time;
                s.step = Step::ReturnWait;
            }
        }
    }

    fn return_moving(&mut self, s: &mut SnufferTwin) {
        match self.walked() {
            None => s.step = Step::Idle,
            Some(false) => {}
            Some(true) => {
                s.start = s.time;
                s.step = Step::ReturnWait;
            }
        }
    }
}

/// Parts of the world digest that are each brain's own: the graph brain's `ai.Brain` and
/// `ai.BtState`, the twin's state, and signals (only the graph brain raises `ai.Decision`).
const OWN_PARTS: [&str; 4] = ["ai.Brain", "ai.BtState", "twin.SnufferTwin", "@signals"];

/// A 40 s harbour night on the committed replay's inputs, the Snuffer thought for by the graph
/// brain or (`twin`) by the Rust twin.
pub(crate) fn night(twin: bool) -> crate::lamplight_world::Night {
    use crate::lamplight_world::{ACTOR, Night};
    let extra: Vec<Arc<dyn EngineModule>> = vec![Arc::new(TwinModule)];
    let mut n = Night::in_scene_with("playing", "scenes/harbour_watch.scene.ron", extra);
    let snuffer = n.named("Snuffer").0;
    if twin {
        n.ok(
            "component.remove",
            serde_json::json!({"entity": snuffer, "component": "ai.Brain", "actor": ACTOR}),
        );
        let state = serde_json::json!({"step": "Idle", "time": 0.0, "start": 0.0, "until": 0.0});
        n.ok("component.set", serde_json::json!({"entity": snuffer, "component": "twin.SnufferTwin", "value": state, "actor": ACTOR}));
    }
    n.keys(&crate::lamplight_snuffer::SCRIPT);
    n
}

/// The world's digest parts, less each brain's own.
pub(crate) fn parts(
    n: &crate::lamplight_world::Night,
) -> std::collections::BTreeMap<String, String> {
    let digest = n
        .engine()
        .world(concordia_api::WorldSel::play)
        .unwrap()
        .state_digest()
        .unwrap();
    digest
        .parts
        .into_iter()
        .filter(|(name, _)| !OWN_PARTS.contains(&name.as_str()))
        .map(|(name, h)| (name, h.to_string()))
        .collect()
}
