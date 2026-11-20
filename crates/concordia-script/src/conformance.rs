//! The backend conformance suite: the same behaviors, checked on every backend against the
//! conformance module (`tests/fixtures/script/conformance-guest`). A backend's tests call
//! [`conformance_suite!`](crate::conformance_suite) with a loader for that module.

#![allow(missing_docs, clippy::missing_panics_doc, clippy::unwrap_used)]

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use concordia_core::id::{DeterministicIdGen, EntityId, IdSource};
use concordia_core::jobs::JobSystem;
use concordia_core::time::TickRate;
use concordia_reflect::{DynamicValue, TypeRegistry};
use concordia_terra::{
    DynamicQuery, NamedQuery, Phase, Schedule, SimClock, SimLoop, World, WorldHash, processor,
};
use concordia_terra_sim::{Template, trait_names};
use serde_json::{Value, json};

use crate::{HostConfig, ScriptHost, ScriptModule, Templates};

/// Loads the conformance module on the backend under test.
pub type Loader = fn() -> Arc<dyn ScriptModule>;

/// The conformance module's processors.
pub const PROCESSORS: [&str; 10] = [
    "conf.integrate",
    "conf.mark_a",
    "conf.mark_b",
    "conf.pong",
    "conf.fault",
    "conf.spawner",
    "conf.burn",
    "conf.nan",
    "conf.wasi",
    "conf.statics",
];

/// The session seed rigs use.
pub const SESSION_SEED: u64 = 42;

/// Expands to one `#[test]` per conformance behavior, each loading the module with `$load`.
#[macro_export]
macro_rules! conformance_suite {
    ($load:expr) => {
        $crate::conformance_suite!(@tests $load;
            describe_registers_processors_components_signals_and_traits,
            batch_delivers_1000_matches_columnar_in_one_call,
            boundary_crossings_per_tick_are_constant_in_entity_count,
            guest_commands_apply_at_phase_end_in_plan_order,
            signals_reach_guests_and_guests_raise_signals,
            fault_is_contained_reported_and_processor_disabled,
            script_trait_composes_into_a_template,
            script_spawns_from_a_template,
            script_processors_hash_identically_at_1_and_8_threads
        );
    };
    (@tests $load:expr; $($name:ident),+) => {
        $(
            #[test]
            fn $name() {
                $crate::conformance::$name($load)
            }
        )+
    };
}

/// A play world running one script module, plus optional Rust processors.
pub struct Rig {
    pub world: World,
    pub registry: TypeRegistry,
    pub host: ScriptHost,
    pub sim: SimLoop,
    pub jobs: JobSystem,
    ids: DeterministicIdGen,
}

/// The `mover` template: the `conf.Mover` trait with velocity (2, 0).
pub fn mover_template() -> Template {
    let mut t = Template::new(
        "mover",
        "A conformance mover.",
        [("conf.Velocity", json!({"x": 2.0, "y": 0.0}))],
    );
    t.traits.insert("conf.Mover".into(), json!({}));
    t
}

impl Rig {
    /// The module alone, one thread.
    pub fn new(load: Loader) -> Self {
        Self::build(load, 1, &[], |_| {})
    }

    /// The module with `templates`, on `threads` threads, plus Rust processors from `extra`.
    pub fn build(
        load: Loader,
        threads: usize,
        templates: &[Template],
        extra: impl FnOnce(&mut Schedule),
    ) -> Self {
        Self::with_host(load(), threads, templates, extra)
    }

    /// As [`Rig::build`], with an already loaded module.
    pub fn with_host(
        module: Arc<dyn ScriptModule>,
        threads: usize,
        templates: &[Template],
        extra: impl FnOnce(&mut Schedule),
    ) -> Self {
        let mut registry = TypeRegistry::default();
        concordia_terra_sim::register(&mut registry).unwrap();
        let config = HostConfig {
            session_seed: SESSION_SEED,
        };
        let host = ScriptHost::load(module, &mut registry, config).expect("module loads");
        let compiled: Templates = templates
            .iter()
            .map(|t| {
                (
                    t.name.clone(),
                    Arc::new(t.compile(&registry).expect("template compiles")),
                )
            })
            .collect();
        let mut schedule = Schedule::new();
        for p in host
            .processors(&registry, &compiled)
            .expect("processors build")
        {
            schedule.add_processor(p);
        }
        extra(&mut schedule);
        let mut world = World::new();
        let sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).expect("schedule plans");
        let jobs = JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap();
        Self {
            world,
            registry,
            host,
            sim,
            jobs,
            ids: DeterministicIdGen::new(7),
        }
    }

    /// Spawns an entity with component values given as JSON.
    pub fn spawn(&mut self, components: &[(&str, Value)]) -> EntityId {
        let id = self.world.spawn(&mut self.ids).unwrap();
        for (name, value) in components {
            self.world
                .set_json(id, name, value.clone(), &self.registry)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
        }
        id
    }

    /// Component `name` of `id` as JSON, if present.
    pub fn get(&self, id: EntityId, name: &str) -> Option<Value> {
        self.world.get_json(id, name).ok()
    }

    /// Entities with every component in `names`.
    pub fn with(&self, names: &[&str]) -> Vec<EntityId> {
        DynamicQuery {
            with: names.iter().map(|n| (*n).to_owned()).collect(),
            without: Vec::new(),
        }
        .run(&self.world, &self.registry)
        .unwrap()
    }

    /// Raises signal `name` on `target` (delivered next tick).
    pub fn signal(&mut self, target: EntityId, name: &str, value: Value) {
        let info = self
            .registry
            .get_by_name(name)
            .expect("signal registered")
            .info();
        self.world
            .signal_dynamic(target, DynamicValue::from_json(info, value).unwrap());
    }

    /// Runs `ticks` ticks.
    pub fn step(&mut self, ticks: u32) {
        self.sim.step(&mut self.world, &self.jobs, ticks);
    }

    /// The world hash.
    pub fn hash(&self) -> WorldHash {
        self.world.state_hash().unwrap()
    }

    /// The simulated seconds of the last tick.
    pub fn dt(&self) -> f32 {
        let clock = self.world.resource::<SimClock>().unwrap();
        (1.0 / f64::from(clock.rate.hz()) * f64::from(clock.time_scale.get())) as f32
    }
}

fn f32_of(value: &Value, field: &str) -> f32 {
    value[field]
        .as_f64()
        .unwrap_or_else(|| panic!("{field} in {value}")) as f32
}

fn movers(rig: &mut Rig, n: usize) -> Vec<EntityId> {
    (0..n)
        .map(|i| {
            rig.spawn(&[
                ("conf.Position", json!({"x": 0.0, "y": 0.0})),
                ("conf.Velocity", json!({"x": i as f32, "y": 1.0})),
            ])
        })
        .collect()
}

pub fn describe_registers_processors_components_signals_and_traits(load: Loader) {
    let rig = Rig::new(load);
    assert_eq!(rig.host.desc().name, "conformance");
    for name in [
        "conf.Position",
        "conf.Velocity",
        "conf.Marker",
        "conf.Faulty",
        "conf.Ping",
        "conf.Pong",
    ] {
        let reg = rig
            .registry
            .get_by_name(name)
            .unwrap_or_else(|| panic!("{name} registered"));
        assert!(reg.is_dynamic(), "{name} is a script type");
        assert!(
            reg.data::<concordia_terra::ReflectComponent>().is_some(),
            "{name} is usable as a component or signal"
        );
        assert!(
            rig.registry.schema_for(name).is_some(),
            "{name} has a schema"
        );
    }
    assert!(trait_names(&rig.registry).any(|t| t == "conf.Mover"));
    let names = rig.sim.schedule().names();
    for p in PROCESSORS {
        assert!(names.contains(&p), "{p} scheduled; have {names:?}");
    }
    let waves = rig.sim.schedule().waves();
    let in_phase = |phase: Phase| -> Vec<&str> {
        waves
            .iter()
            .filter(|(p, _)| *p == phase)
            .flat_map(|(_, w)| w.iter().flatten().copied())
            .collect()
    };
    assert!(in_phase(Phase::PostPhysics).contains(&"conf.integrate"));
    let frame_end = in_phase(Phase::FrameEnd);
    let a = frame_end.iter().position(|p| *p == "conf.mark_a").unwrap();
    let b = frame_end.iter().position(|p| *p == "conf.mark_b").unwrap();
    assert!(a < b, "mark_a runs before mark_b: {frame_end:?}");
}

pub fn batch_delivers_1000_matches_columnar_in_one_call(load: Loader) {
    let mut rig = Rig::new(load);
    let ids = movers(&mut rig, 1000);
    let before = rig.host.stats();
    rig.step(1);
    let calls = rig.host.stats().calls_in - before.calls_in;
    assert_eq!(
        calls,
        PROCESSORS.len() as u64,
        "one call per processor, not per entity"
    );
    let dt = rig.dt();
    for (i, id) in ids.iter().enumerate() {
        let p = rig.get(*id, "conf.Position").unwrap();
        assert_eq!(f32_of(&p, "x"), 0.0 + i as f32 * dt, "row {i}");
        assert_eq!(f32_of(&p, "y"), dt, "row {i}");
    }
}

pub fn boundary_crossings_per_tick_are_constant_in_entity_count(load: Loader) {
    let crossings = |n: usize| {
        let mut rig = Rig::new(load);
        movers(&mut rig, n);
        rig.step(1);
        let before = rig.host.stats().crossings();
        rig.step(1);
        rig.host.stats().crossings() - before
    };
    let small = crossings(10);
    assert_eq!(small, crossings(1000));
    assert_eq!(small, PROCESSORS.len() as u64);
}

pub fn guest_commands_apply_at_phase_end_in_plan_order(load: Loader) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let mut rig = Rig::build(load, 8, &[], move |s| {
        s.add_processor(
            processor("test.watch", Phase::FrameEnd)
                .reads_named("conf.Marker")
                .after("conf.mark_b")
                .run(move |view, _| {
                    let batch = view.batch(&NamedQuery::fetch(["conf.Marker"])).unwrap();
                    sink.lock().unwrap().push(batch.ids.len());
                }),
        );
    });
    let id = rig.spawn(&[("conf.Counter", json!({"n": 0}))]);
    rig.step(1);
    assert_eq!(
        *seen.lock().unwrap(),
        [0],
        "commands wait for the phase to end"
    );
    assert_eq!(
        rig.get(id, "conf.Marker"),
        Some(json!({"by": "b"})),
        "plan order: b after a"
    );
    rig.step(1);
    assert_eq!(*seen.lock().unwrap(), [0, 1]);
}

pub fn signals_reach_guests_and_guests_raise_signals(load: Loader) {
    let got = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&got);
    let mut rig = Rig::build(load, 1, &[], move |s| {
        s.add_processor(
            processor("test.pongs", Phase::PostPhysics)
                .receives_named("conf.Pong")
                .run(move |view, _| {
                    for (id, pong) in view.signals_named("conf.Pong").unwrap() {
                        sink.lock().unwrap().push((id, pong.to_json().unwrap()));
                    }
                }),
        );
    });
    let target = rig.spawn(&[("conf.Counter", json!({"n": 1}))]);
    rig.signal(target, "conf.Ping", json!({"amount": 41}));
    rig.step(1);
    assert!(
        got.lock().unwrap().is_empty(),
        "a raised signal arrives next tick"
    );
    rig.step(1);
    assert_eq!(*got.lock().unwrap(), [(target, json!({"amount": 42}))]);
}

pub fn fault_is_contained_reported_and_processor_disabled(load: Loader) {
    let mut rig = Rig::new(load);
    let mover = movers(&mut rig, 2)[1];
    rig.spawn(&[("conf.Faulty", json!(null))]);
    rig.step(1);
    let faults = rig.host.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(faults[0].processor, "conf.fault");
    assert_eq!(faults[0].actor, "script:conformance");
    assert_eq!(faults[0].tick, 0, "the first tick");
    assert!(
        matches!(faults[0].fault, crate::ScriptFault::Trap(_)),
        "{:?}",
        faults[0].fault
    );
    assert!(rig.host.is_disabled("conf.fault"));
    let logs = rig.host.logs();
    assert!(
        logs.iter()
            .any(|l| l.message.contains("about to trap") && l.actor == "script:conformance"),
        "the faulting run's log survives: {logs:?}"
    );
    assert!(
        f32_of(&rig.get(mover, "conf.Position").unwrap(), "x") > 0.0,
        "the rest ran"
    );
    let before = rig.host.stats().calls_in;
    rig.step(1);
    assert_eq!(
        rig.host.stats().calls_in - before,
        PROCESSORS.len() as u64 - 1
    );
    assert_eq!(
        rig.host.faults().len(),
        1,
        "a disabled processor isn't called again"
    );
}

pub fn script_trait_composes_into_a_template(load: Loader) {
    let mut rig = Rig::new(load);
    let compiled = mover_template().compile(&rig.registry).unwrap();
    assert_eq!(
        compiled.component_names(),
        ["conf.Position", "conf.Velocity"]
    );
    assert_eq!(compiled.requires(), ["conf.integrate"]);
    assert!(
        compiled
            .missing_processors(rig.sim.schedule().names())
            .is_empty()
    );
    let id = rig.ids.next_id().unwrap();
    compiled.spawn(&mut rig.world, id).unwrap();
    assert_eq!(
        rig.get(id, "conf.Position"),
        Some(json!({"x": 0.0, "y": 0.0})),
        "trait default"
    );
    rig.step(1);
    assert_eq!(
        f32_of(&rig.get(id, "conf.Position").unwrap(), "x"),
        2.0 * rig.dt()
    );
}

pub fn script_spawns_from_a_template(load: Loader) {
    let mut rig = Rig::build(load, 1, &[mover_template()], |_| {});
    let request = rig.spawn(&[("conf.SpawnRequest", json!({"template": "mover"}))]);
    rig.step(1);
    assert!(
        !rig.world.contains(request),
        "the guest despawned its request"
    );
    let spawned = rig.with(&["conf.Position", "conf.Velocity"]);
    assert_eq!(spawned.len(), 1);
    assert_eq!(
        rig.get(spawned[0], "conf.Velocity"),
        Some(json!({"x": 2.0, "y": 0.0}))
    );
    rig.spawn(&[("conf.SpawnRequest", json!({"template": "missing"}))]);
    rig.step(1);
    let faults = rig.host.faults();
    assert_eq!(faults.len(), 1);
    assert!(
        faults[0].fault.to_string().contains("missing"),
        "{faults:?}"
    );
    assert_eq!(
        rig.with(&["conf.Position"]).len(),
        1,
        "a faulted run spawns nothing"
    );
}

/// A mixed scenario (movers, signals, markers, template spawns) on `threads` threads; the
/// world hash after ticks 1, 10 and 30.
pub fn scenario(load: Loader, threads: usize) -> Vec<WorldHash> {
    let mut rig = Rig::build(load, threads, &[mover_template()], |_| {});
    let ids = movers(&mut rig, 300);
    for (i, id) in ids.iter().enumerate().step_by(7) {
        rig.signal(*id, "conf.Ping", json!({"amount": i}));
        rig.world
            .set_json(*id, "conf.Counter", json!({"n": i}), &rig.registry)
            .unwrap();
    }
    for _ in 0..5 {
        rig.spawn(&[("conf.SpawnRequest", json!({"template": "mover"}))]);
    }
    let mut hashes = Vec::new();
    for ticks in [1, 9, 20] {
        rig.step(ticks);
        hashes.push(rig.hash());
    }
    hashes
}

pub fn script_processors_hash_identically_at_1_and_8_threads(load: Loader) {
    let one = scenario(load, 1);
    assert_eq!(one, scenario(load, 8));
    assert_eq!(one, scenario(load, 1), "and run to run");
}

/// Every conformance behavior in turn (for a backend's single umbrella test).
pub fn run_all(load: Loader) {
    describe_registers_processors_components_signals_and_traits(load);
    batch_delivers_1000_matches_columnar_in_one_call(load);
    boundary_crossings_per_tick_are_constant_in_entity_count(load);
    guest_commands_apply_at_phase_end_in_plan_order(load);
    signals_reach_guests_and_guests_raise_signals(load);
    fault_is_contained_reported_and_processor_disabled(load);
    script_trait_composes_into_a_template(load);
    script_spawns_from_a_template(load);
    script_processors_hash_identically_at_1_and_8_threads(load);
}
