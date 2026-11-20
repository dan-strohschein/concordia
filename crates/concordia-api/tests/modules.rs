//! Engine modules: types, systems, methods and play-time state contributed from outside the crate (M1 phase 1.0).

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::Harness;
use concordia_api::{
    ApiError, ApiErrorKind, CallCtx, Engine, EngineModule, Method, MethodInfo, PlaySetup, WorldSel,
};
use concordia_reflect::{Reflect, RegistryError, TypeRegistry};
use concordia_scene::builtin::Transform;
use concordia_terra::{Schedule, SystemCtx, World, register_component, system};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Hit points (a module-provided component).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(transparent)]
struct Health(u32);

/// Counts play sessions; `Drop` counts when the engine discards the state.
#[derive(Debug)]
struct ProbeState {
    value: u32,
    dropped: Arc<AtomicUsize>,
}

impl Drop for ProbeState {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

/// Positions seen by the module's system (to check it runs after `integrate_velocity`).
#[derive(Debug, Clone, Default, PartialEq)]
struct Seen(Vec<f32>);

/// Marker a module puts into each new play world.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Prepared(usize);

/// Return the probe module's play state value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
struct ProbeValue;

/// `probe.value` params.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
struct ProbeParams {}

/// `probe.value` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
struct ProbeResult {
    /// The value.
    value: u32,
}

impl Method for ProbeValue {
    const NAME: &'static str = "probe.value";
    const MUTATES: bool = false;
    type Params = ProbeParams;
    type Result = ProbeResult;

    fn call(engine: &mut Engine, _: &CallCtx, _: ProbeParams) -> Result<ProbeResult, ApiError> {
        let state = engine
            .play_state::<ProbeState>()
            .ok_or_else(|| ApiError::from_kind(ApiErrorKind::NotPlaying))?;
        Ok(ProbeResult { value: state.value })
    }
}

#[derive(Debug, Default)]
struct Probe {
    started: Arc<AtomicUsize>,
    dropped: Arc<AtomicUsize>,
}

impl EngineModule for Probe {
    fn name(&self) -> &'static str {
        "probe"
    }

    fn register_types(&self, registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        register_component::<Health>(registry)
    }

    fn methods(&self) -> Vec<MethodInfo> {
        vec![MethodInfo::of::<ProbeValue>()]
    }

    fn start_play(&self, world: &mut World) -> Result<PlaySetup, ApiError> {
        let n = self.started.fetch_add(1, Ordering::SeqCst) + 1;
        world.insert_resource(Prepared(n));
        let record = system("probe.record", |world: &mut World, _: &SystemCtx<'_>| {
            let mut xs = Vec::new();
            if let Ok(mut q) = world.query::<&Transform>() {
                q.for_each(world, |_, t| xs.push(t.translation.x));
            }
            let seen = world.resource::<Seen>().cloned().unwrap_or_default();
            world.insert_resource(Seen([seen.0, xs].concat()));
        });
        let mut schedule = Schedule::new();
        schedule.add(record);
        Ok(PlaySetup {
            schedule,
            state: Some(Box::new(ProbeState {
                value: 40 + n as u32,
                dropped: Arc::clone(&self.dropped),
            })),
        })
    }
}

/// A module that claims a method name another module already has.
#[derive(Debug)]
struct Clash(&'static str);

impl EngineModule for Clash {
    fn name(&self) -> &'static str {
        "clash"
    }

    fn methods(&self) -> Vec<MethodInfo> {
        match self.0 {
            "probe.value" => vec![MethodInfo::of::<ProbeValue>()],
            _ => Vec::new(),
        }
    }
}

fn with_modules(modules: Vec<Arc<dyn EngineModule>>) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    Harness::with_config(dir, move |c| c.modules = modules).ready()
}

#[test]
fn module_types_are_registered_and_schema_visible() {
    let mut h = with_modules(vec![Arc::new(Probe::default())]);
    let schema = h.ok("schema.get", json!({"name": "Health"}));
    assert_eq!(schema["schema"]["type"], "integer", "{schema}");
    let spawned = h.ok("entity.spawn", json!({"components": {"Health": 7}}));
    let got = h.ok(
        "component.get",
        json!({"entity": spawned["id"], "component": "Health"}),
    );
    assert_eq!(got["value"], 7);
}

#[test]
fn module_methods_dispatch_and_appear_in_schema_index() {
    let mut h = with_modules(vec![Arc::new(Probe::default())]);
    let index = h.ok("schema.get", json!({}));
    let names: Vec<&str> = index["schema"]["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap())
        .collect();
    assert!(names.contains(&"probe.value"), "{names:?}");
    assert!(names.windows(2).all(|w| w[0] < w[1]), "sorted: {names:?}");
    assert!(h.engine.methods().get("probe.value").is_some());
    let described = h.ok("schema.get", json!({"name": "probe.value"}));
    assert_eq!(described["schema"]["mutates"], false);
}

#[test]
fn core_methods_are_unchanged_without_modules() {
    let h = Harness::new();
    let core: Vec<&str> = concordia_api::methods().names().collect();
    let engine: Vec<&str> = h.engine.methods().names().collect();
    assert_eq!(engine, core);
}

#[test]
fn duplicate_method_name_across_modules_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = common::config(&dir);
    config.modules = vec![Arc::new(Probe::default()), Arc::new(Clash("probe.value"))];
    let err = Engine::new(config).expect_err("duplicate method");
    assert!(err.message.contains("probe.value"), "{}", err.message);
    assert!(err.message.contains("clash"), "{}", err.message);
}

#[test]
fn duplicate_module_name_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = common::config(&dir);
    config.modules = vec![Arc::new(Clash("none")), Arc::new(Clash("none"))];
    let err = Engine::new(config).expect_err("duplicate module");
    assert!(err.message.contains("clash"), "{}", err.message);
}

#[test]
fn module_systems_run_after_core_systems_in_declared_order() {
    let mut h = with_modules(vec![Arc::new(Probe::default())]);
    let components = json!({
        "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
        "Velocity": {"linear": [60.0, 0.0, 0.0], "angular": [0.0, 0.0, 0.0]},
    });
    h.ok("entity.spawn", json!({ "components": components }));
    h.ok("sim.step", json!({"ticks": 2}));
    let seen = h.engine.world(WorldSel::play).unwrap().resource::<Seen>();
    // integrate_velocity (core) runs first, so the module sees x = 1, then 2.
    assert_eq!(seen, Some(&Seen(vec![1.0, 2.0])));
}

#[test]
fn play_state_is_created_on_play_and_dropped_on_stop_and_scene_open() {
    let probe = Arc::new(Probe::default());
    let (started, dropped) = (Arc::clone(&probe.started), Arc::clone(&probe.dropped));
    let mut h = with_modules(vec![probe]);
    assert_eq!(started.load(Ordering::SeqCst), 0);
    h.ok("sim.step", json!({"ticks": 1}));
    h.ok("sim.step", json!({"ticks": 1}));
    assert_eq!(started.load(Ordering::SeqCst), 1, "one play session");
    h.ok("sim.stop", json!({}));
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    h.ok("sim.play", json!({"rate_hz": 30}));
    h.ok("sim.play", json!({"rate_hz": 120}));
    assert_eq!(
        started.load(Ordering::SeqCst),
        2,
        "a rate change keeps the session"
    );
    h.ok("scene.save", json!({}));
    h.ok("scene.open", json!({"path": "scenes/test.scene.ron"}));
    assert_eq!(dropped.load(Ordering::SeqCst), 2);
}

#[test]
fn module_method_reads_play_state() {
    let mut h = with_modules(vec![Arc::new(Probe::default())]);
    let err = h.err("probe.value", json!({}));
    assert_eq!(err.data.kind, "NotPlaying");
    h.ok("sim.step", json!({"ticks": 1}));
    assert_eq!(h.ok("probe.value", json!({}))["value"], 41);
}

#[test]
fn start_play_can_prepare_the_play_world_but_not_the_edit_world() {
    let mut h = with_modules(vec![Arc::new(Probe::default())]);
    h.ok("sim.step", json!({"ticks": 1}));
    let play = h.engine.world(WorldSel::play).unwrap();
    assert_eq!(play.resource::<Prepared>(), Some(&Prepared(1)));
    let edit = h.engine.world(WorldSel::edit).unwrap();
    assert_eq!(edit.resource::<Prepared>(), None);
}

#[test]
fn play_world_mut_creates_a_paused_play_world() {
    let mut h = with_modules(vec![Arc::new(Probe::default())]);
    assert!(h.engine.world(WorldSel::play).is_err());
    let world = h.engine.play_world_mut().unwrap();
    assert_eq!(world.resource::<Prepared>(), Some(&Prepared(1)));
    assert_eq!(h.ok("sim.pause", json!({}))["state"], "paused");
}

/// Engine-lifetime state of the counter module.
#[derive(Debug, Default)]
struct Counter(u32);

/// Increments the counter module's engine state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
struct Bump;

impl Method for Bump {
    const NAME: &'static str = "counter.bump";
    const MUTATES: bool = false;
    type Params = ProbeParams;
    type Result = ProbeResult;

    fn call(engine: &mut Engine, _: &CallCtx, _: ProbeParams) -> Result<ProbeResult, ApiError> {
        let counter = engine
            .state_mut::<Counter>()
            .ok_or_else(|| ApiError::from_kind(ApiErrorKind::Internal))?;
        counter.0 += 1;
        Ok(ProbeResult { value: counter.0 })
    }
}

#[derive(Debug)]
struct CounterModule;

impl EngineModule for CounterModule {
    fn name(&self) -> &'static str {
        "counter"
    }

    fn methods(&self) -> Vec<MethodInfo> {
        vec![MethodInfo::of::<Bump>()]
    }

    fn engine_state(&self) -> Option<Box<dyn std::any::Any + Send>> {
        Some(Box::new(Counter::default()))
    }
}

#[test]
fn engine_state_lives_for_the_engine_and_survives_play_sessions() {
    let mut h = with_modules(vec![Arc::new(CounterModule)]);
    assert_eq!(h.ok("counter.bump", json!({}))["value"], 1);
    h.ok("sim.step", json!({"ticks": 1}));
    h.ok("sim.stop", json!({}));
    assert_eq!(h.ok("counter.bump", json!({}))["value"], 2);
    assert!(h.engine.state::<Counter>().is_some());
    assert!(h.engine.state::<Seen>().is_none());
}

/// Counts `after_sim` calls.
#[derive(Debug)]
struct SimWatcher(Arc<AtomicUsize>);

impl EngineModule for SimWatcher {
    fn name(&self) -> &'static str {
        "sim-watcher"
    }

    fn after_sim(&self, _: &mut Engine) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn modules_hear_about_each_simulation_advance() {
    let seen = Arc::new(AtomicUsize::new(0));
    let mut h = with_modules(vec![Arc::new(SimWatcher(seen.clone()))]).ready();
    h.ok("sim.play", json!({}));
    let start = std::time::Instant::now();
    h.engine.poll(start + std::time::Duration::from_millis(100));
    let after_play = seen.load(Ordering::SeqCst);
    assert!(after_play >= 1, "a tick advanced");
    h.engine.poll(start + std::time::Duration::from_millis(100));
    assert_eq!(seen.load(Ordering::SeqCst), after_play, "no tick, no call");
    h.ok("sim.pause", json!({}));
    h.engine.poll(start + std::time::Duration::from_millis(500));
    assert_eq!(seen.load(Ordering::SeqCst), after_play, "paused");
}

/// What a play session's module saw when it started.
#[derive(Debug, Clone, PartialEq)]
struct SawEngine {
    seed: u64,
    knows_health: bool,
}

/// A module that reads the engine when play starts.
#[derive(Debug)]
struct Seer;

impl EngineModule for Seer {
    fn name(&self) -> &'static str {
        "seer"
    }

    fn register_types(&self, registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        register_component::<Health>(registry)
    }

    fn start_play_in(&self, engine: &Engine, world: &mut World) -> Result<PlaySetup, ApiError> {
        world.insert_resource(SawEngine {
            seed: engine.sim_seed(),
            knows_health: engine.registry().get_by_name("Health").is_some(),
        });
        Ok(PlaySetup::default())
    }
}

#[test]
fn start_play_in_sees_the_engine_registry_and_seed() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::with_config(dir, |c| {
        c.modules = vec![Arc::new(Seer)];
        c.sim_seed = 99;
    })
    .ready();
    h.ok("sim.step", json!({"ticks": 1}));
    let play = h.engine.world(WorldSel::play).unwrap();
    assert_eq!(
        play.resource::<SawEngine>(),
        Some(&SawEngine {
            seed: 99,
            knows_health: true
        })
    );
    assert!(h.engine.template_catalog().is_ok());
}

#[test]
fn commit_edits_as_records_its_actor_and_undo_as_reverts_it() {
    let mut h = Harness::new().ready();
    let id = h.spawn("A", 1.0);
    let hash = |h: &Harness| {
        h.engine
            .world(WorldSel::edit)
            .unwrap()
            .state_hash()
            .unwrap()
    };
    let before = hash(&h);
    let actor: concordia_command::Actor = "script:mod".parse().unwrap();
    let op = concordia_command::EditOp::RemoveComponent {
        id: id.parse().unwrap(),
        component: "Name".into(),
    };
    let txn = h
        .engine
        .commit_edits_as(actor.clone(), "script edit", vec![op])
        .unwrap()
        .expect("a transaction");
    let history = h.ok("txn.history", json!({"limit": 1}));
    assert_eq!(history["entries"][0]["actor"], "script:mod", "{history}");
    assert_eq!(history["entries"][0]["label"], "script edit");
    assert_eq!(
        h.engine
            .commit_edits_as(actor.clone(), "nothing", Vec::new())
            .unwrap(),
        None
    );

    h.engine.undo_as(actor.clone(), txn).unwrap();
    assert_eq!(hash(&h), before);
    let history = h.ok("txn.history", json!({"limit": 1}));
    assert_eq!(history["entries"][0]["actor"], "script:mod", "{history}");

    h.ok("txn.begin", json!({"label": "mine"}));
    let held = h.engine.commit_edits_as(
        actor,
        "blocked",
        vec![concordia_command::EditOp::Despawn {
            id: id.parse().unwrap(),
        }],
    );
    assert_eq!(held.unwrap_err().kind, ApiErrorKind::TxnConflict);
}
