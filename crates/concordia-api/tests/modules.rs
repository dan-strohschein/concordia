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
