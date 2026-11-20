//! A test engine: input, a `steer` module (the `camera_pan` action drives every Velocity) and
//! replay, on a temporary project with three moving entities.
#![allow(dead_code)]

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use concordia_api::{ApiError, Engine, EngineConfig, EngineModule, Fingerprint, PlaySetup};
use concordia_core::id::SessionId;
use concordia_core::log::LogRing;
use concordia_core::math::Vec3;
use concordia_input::{ActionStates, ActionValue, InputMap, InputModule};
use concordia_replay::ReplayModule;
use concordia_scene::builtin::{Transform, Velocity};
use concordia_terra::{Phase, Schedule, SystemCtx, World, system};
use serde_json::{Value, json};

pub const MAP: &str = include_str!("../../../../examples/m1-physics-stack/input/default.input.ron");

/// Steers every entity from `camera_pan`; can perturb one Transform on a chosen tick.
#[derive(Debug)]
pub struct Steer {
    /// The tick (`SystemCtx::tick`) to nudge the first entity on; `u64::MAX` for never.
    pub perturb_at: Arc<AtomicU64>,
    /// The fingerprint hash the module reports.
    pub code: Arc<Mutex<String>>,
    /// Makes the next play worlds fail to start.
    pub fail_start: Arc<std::sync::atomic::AtomicBool>,
}

impl Default for Steer {
    fn default() -> Self {
        Self {
            perturb_at: Arc::new(AtomicU64::new(u64::MAX)),
            code: Arc::new(Mutex::new("steer-v1".into())),
            fail_start: Arc::default(),
        }
    }
}

impl EngineModule for Steer {
    fn name(&self) -> &'static str {
        "steer"
    }

    fn fingerprints(&self) -> Vec<Fingerprint> {
        vec![Fingerprint {
            module: "steer".into(),
            name: "steer".into(),
            kind: "test".into(),
            hash: self.code.lock().unwrap().clone(),
        }]
    }

    fn start_play(&self, _: &mut World) -> Result<PlaySetup, ApiError> {
        if self.fail_start.load(Ordering::SeqCst) {
            return Err(ApiError::new(
                concordia_api::ApiErrorKind::Internal,
                "steer failed",
            ));
        }
        let perturb = Arc::clone(&self.perturb_at);
        let mut schedule = Schedule::new();
        schedule.add_in(
            Phase::PrePhysics,
            system("steer.apply", move |world: &mut World, ctx: &SystemCtx| {
                let pan = match world
                    .resource::<ActionStates>()
                    .and_then(|s| s.get("camera_pan"))
                    .map(|s| s.value)
                {
                    Some(ActionValue::Axis2d(v)) => v,
                    _ => Default::default(),
                };
                let mut query = world.query::<&mut Velocity>().unwrap();
                query.for_each(world, |_, v| v.linear = Vec3::new(pan.x, 0.0, pan.y) * 2.0);
                if ctx.tick.0 == perturb.load(Ordering::SeqCst) {
                    let first = world.entity_ids_sorted()[0];
                    if let Some(t) = world.get_mut::<Transform>(first) {
                        t.translation.x += 0.25;
                    }
                }
            }),
        );
        Ok(PlaySetup {
            schedule,
            state: None,
        })
    }
}

/// An engine with a scene of three moving entities, a session as `agent:qa`.
pub struct Rig {
    pub dir: tempfile::TempDir,
    pub engine: Engine,
    pub session: SessionId,
    pub steer: Arc<Steer>,
}

/// Engine settings a test may change.
pub struct Opts {
    pub threads: usize,
    pub seed: u64,
    pub ring: Option<Arc<LogRing>>,
    pub extra: Vec<Arc<dyn EngineModule>>,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            threads: 1,
            seed: 5,
            ring: None,
            extra: Vec::new(),
        }
    }
}

impl Rig {
    pub fn new() -> Self {
        Self::with(Opts::default())
    }

    pub fn with(opts: Opts) -> Self {
        let dir = tempfile::tempdir().unwrap();
        Self::in_dir(dir, opts)
    }

    /// A rig on `dir` (e.g. another rig's project, to replay its files).
    pub fn in_dir(dir: tempfile::TempDir, opts: Opts) -> Self {
        let steer = Arc::new(Steer::default());
        let mut config = EngineConfig::new(dir.path());
        config.deterministic_ids = Some(7);
        config.threads = NonZeroUsize::new(opts.threads).unwrap();
        config.journal.sync = false;
        config.sim_seed = opts.seed;
        config.log_ring = opts.ring;
        let input: Arc<dyn EngineModule> =
            Arc::new(InputModule::new(InputMap::parse(MAP).unwrap()));
        let mut modules = vec![input, Arc::clone(&steer) as Arc<dyn EngineModule>];
        modules.extend(opts.extra);
        modules.push(Arc::new(ReplayModule::new()));
        config.modules = modules;
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut rig = Self {
            dir,
            engine,
            session,
            steer,
        };
        rig.ok(
            "session.open",
            json!({"client": "test", "actor": "agent:qa"}),
        );
        let scene = rig.dir.path().join("scenes/test.scene.ron");
        if scene.exists() {
            rig.ok("scene.open", json!({"path": "scenes/test.scene.ron"}));
        } else {
            rig.ok(
                "scene.new",
                json!({"path": "scenes/test.scene.ron", "name": "Test"}),
            );
            for i in 0..3 {
                rig.ok(
                    "entity.spawn",
                    json!({"components": {
                        "Name": format!("Mover {i}"),
                        "Transform": {"translation": [i as f32, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
                        "Velocity": {"linear": [0.0, 0.0, 0.0], "angular": [0.0, 0.0, 0.0]},
                    }}),
                );
            }
            rig.ok("scene.save", json!({}));
        }
        rig
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, ApiError> {
        self.engine.call(self.session, method, params)
    }

    pub fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    pub fn err(&mut self, method: &str, params: Value) -> ApiError {
        match self.call(method, params.clone()) {
            Ok(v) => panic!("{method} {params} should fail, got {v}"),
            Err(e) => e,
        }
    }

    /// Records `ticks` ticks to `path` with `events` (`input.inject` events) and returns the
    /// stop result and the last `sim.step` hash.
    pub fn record(&mut self, path: &str, every: u32, events: Value, ticks: u32) -> (Value, String) {
        self.ok(
            "replay.record",
            json!({"action": "start", "path": path, "checkpoint_every": every}),
        );
        self.ok("input.inject", json!({ "events": events }));
        let hash = self.ok("sim.step", json!({ "ticks": ticks }))["world_hash"]
            .as_str()
            .unwrap()
            .to_owned();
        let stopped = self.ok("replay.record", json!({"action": "stop"}));
        (stopped, hash)
    }

    pub fn file(&self, rel: &str) -> String {
        std::fs::read_to_string(self.dir.path().join(rel)).unwrap()
    }
}

/// `input.inject` events: hold W (pan up) on ticks 10..50, D on 30..90, and a click at 100.
pub fn steering() -> Value {
    let key = |tick: u64, key: &str, pressed: bool| json!({"tick": tick, "event": {"Key": {"key": key, "pressed": pressed}}});
    let click = |tick: u64, pressed: bool| json!({"tick": tick, "event": {"MouseButton": {"button": "Left", "pressed": pressed}}});
    json!([
        key(10, "W", true),
        key(30, "D", true),
        key(50, "W", false),
        key(90, "D", false),
        click(100, true),
        click(100, false),
    ])
}
