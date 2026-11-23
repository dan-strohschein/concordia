//! An engine with `[assets]`, `[audio]` and a gameplay test module (`Bell`: raise `PlayCue` on
//! a tick), over the Engine API. Sounds are written by `hound`: `dc.wav` is 4,800 frames of
//! 16384 (0.5), `short.wav` 100 frames of 0.5, both mono at 48 kHz. Cues: `loop.cue.ron` (dc,
//! looping), `once.cue.ron` (short) and `near.cue.ron` (dc, looping, spatial 1–50 m), all on SFX.
//! Every asset gets a sidecar with a fixed id before import, so two rigs hold the same values
//! (an emitter's `cue` is part of the world hash).
#![allow(dead_code, missing_docs)]

use std::io::Cursor;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;

use concordia_api::{Engine, EngineConfig, EngineModule, PlaySetup};
use concordia_assets::{AssetRef, AssetsModule};
use concordia_audio::{AudioModule, AudioSettings, AudioShared, Backend, PlayCue};
use concordia_core::id::SessionId;
use concordia_reflect::{Reflect, RegistryError, TypeRegistry};
use concordia_terra::{Phase, SystemCtx, View, World, processor, register_component};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const ACTOR: &str = "agent:test";

const PINNED: [&str; 6] = [
    "sfx/dc.wav",
    "sfx/short.wav",
    "sfx/loop.cue.ron",
    "sfx/once.cue.ron",
    "sfx/near.cue.ron",
    "sfx/extra.cue.ron",
];

/// Gameplay's test bell: raises `PlayCue { cue }` on its entity once per entry of `at` naming
/// the tick (so `[2, 2]` rings twice on tick 2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "test.Bell")]
pub struct Bell {
    pub cue: AssetRef,
    pub at: Vec<u64>,
}

/// Gameplay's test hush: raises `StopCue { cue: None }` on its entity on tick `at`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "test.Hush")]
pub struct Hush {
    pub at: u64,
}

/// Gameplay's test doom: despawns its entity on tick `at`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "test.Doom")]
pub struct Doom {
    pub at: u64,
}

#[derive(Debug)]
pub struct Bells;

impl EngineModule for Bells {
    fn name(&self) -> &'static str {
        "bells"
    }

    fn register_types(&self, registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        register_component::<Bell>(registry)?;
        register_component::<Hush>(registry)?;
        register_component::<Doom>(registry)
    }

    fn start_play(&self, _: &mut World) -> Result<PlaySetup, concordia_api::ApiError> {
        let mut setup = PlaySetup::default();
        setup.schedule.add_processor(
            processor("test.ring", Phase::PostPhysics)
                .reads::<Bell>()
                .reads::<Hush>()
                .run(|view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                    let mut hushes = Vec::new();
                    view.query::<&Hush>().unwrap().for_each(|id, hush| {
                        if hush.at == ctx.tick.0 {
                            hushes.push(id);
                        }
                    });
                    let mut rings = Vec::new();
                    view.query::<&Bell>().unwrap().for_each(|id, bell| {
                        for _ in bell.at.iter().filter(|t| **t == ctx.tick.0) {
                            rings.push((id, bell.cue.clone()));
                        }
                    });
                    let commands = view.commands().unwrap();
                    for (id, cue) in rings {
                        commands.signal(
                            id,
                            PlayCue {
                                cue,
                                volume_db: 0.0,
                            },
                        );
                    }
                    for id in hushes {
                        commands.signal(id, concordia_audio::StopCue { cue: None });
                    }
                }),
        );
        setup.schedule.add_processor(
            processor("test.doom", Phase::PostPhysics)
                .reads::<Doom>()
                .run(|view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                    let mut doomed = Vec::new();
                    view.query::<&Doom>().unwrap().for_each(|id, doom| {
                        if doom.at == ctx.tick.0 {
                            doomed.push(id);
                        }
                    });
                    let commands = view.commands().unwrap();
                    for id in doomed {
                        commands.despawn(id);
                    }
                }),
        );
        Ok(setup)
    }
}

pub struct Rig {
    pub dir: tempfile::TempDir,
    pub engine: Engine,
    session: SessionId,
    pub cues: Cues,
}

#[derive(Debug, Clone)]
pub struct Cues {
    pub looped: String,
    pub once: String,
    pub near: String,
    /// dc, looping, volume drawn from −12..0 dB (seed 5).
    pub vary: String,
}

pub fn wav(path: &Path, frames: usize, value: i16) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut bytes = Cursor::new(Vec::new());
    let mut w = hound::WavWriter::new(&mut bytes, spec).unwrap();
    for _ in 0..frames {
        w.write_sample(value).unwrap();
    }
    w.finalize().unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes.into_inner()).unwrap();
}

impl Rig {
    pub fn new(threads: usize, backend: Backend) -> Self {
        Self::with(threads, backend, Vec::new())
    }

    pub fn with(threads: usize, backend: Backend, extra: Vec<Arc<dyn EngineModule>>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        wav(&root.join("sfx/dc.wav"), 4800, 16384);
        wav(&root.join("sfx/short.wav"), 100, 16384);
        let mut config = EngineConfig::new(root);
        config.deterministic_ids = Some(3);
        config.threads = NonZeroUsize::new(threads).unwrap();
        config.journal.sync = false;
        let assets = concordia_audio::importers()
            .into_iter()
            .fold(AssetsModule::new(root), AssetsModule::with_importer);
        let audio = AudioModule::new(AudioSettings {
            backend,
            ..AudioSettings::default()
        })
        .unwrap();
        config.modules = vec![Arc::new(assets), Arc::new(audio), Arc::new(Bells)];
        config.modules.extend(extra);
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut rig = Self {
            dir,
            engine,
            session,
            cues: Cues {
                looped: String::new(),
                once: String::new(),
                near: String::new(),
                vary: String::new(),
            },
        };
        rig.ok("session.open", json!({"client": "test", "actor": ACTOR}));
        rig.ok(
            "scene.new",
            json!({"path": "scenes/a.scene.ron", "name": "A"}),
        );
        let dc = rig.import("sfx/dc.wav");
        let short = rig.import("sfx/short.wav");
        rig.cues = Cues {
            looped: rig.cue("sfx/loop.cue.ron", &format!("sounds: [(sound: \"{dc}\")], looping: true")),
            once: rig.cue("sfx/once.cue.ron", &format!("sounds: [(sound: \"{short}\")]")),
            near: rig.cue(
                "sfx/near.cue.ron",
                &format!("sounds: [(sound: \"{dc}\")], looping: true, spatial: Some((min_distance: 1.0, max_distance: 50.0))"),
            ),
            vary: rig.cue(
                "sfx/extra.cue.ron",
                &format!("sounds: [(sound: \"{dc}\")], looping: true, volume_db: (min: -12.0, max: 0.0), seed: 5"),
            ),
        };
        rig
    }

    /// Writes `path`'s sidecar with a fixed id, so every rig imports it under the same id.
    pub fn pin(&self, path: &str, importer: &str) {
        let n = PINNED
            .iter()
            .position(|p| *p == path)
            .expect("a pinned path")
            + 1;
        let sidecar = format!(
            "Sidecar(id: \"0190b5c0-0000-7000-8000-{n:012x}\", importer: \"{importer}\", importer_version: 1, settings: {{}}, sub_assets: {{}})\n"
        );
        std::fs::write(self.dir.path().join(format!("{path}.import.ron")), sidecar).unwrap();
    }

    pub fn import(&mut self, path: &str) -> String {
        let importer = if path.ends_with(".cue.ron") {
            "cue"
        } else {
            "sound"
        };
        self.pin(path, importer);
        let id = self.ok("asset.import", json!({"path": path, "actor": ACTOR}))["id"]
            .as_str()
            .unwrap()
            .to_owned();
        format!("asset:{id}")
    }

    pub fn cue(&mut self, path: &str, body: &str) -> String {
        std::fs::write(self.dir.path().join(path), format!("({body})\n")).unwrap();
        self.import(path)
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, concordia_api::ApiError> {
        self.engine.call(self.session, method, params)
    }

    pub fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    pub fn spawn(&mut self, components: Value) -> String {
        self.ok("entity.spawn", json!({ "components": components }))["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// Steps one tick and returns the world hash.
    pub fn step(&mut self) -> String {
        self.ok("sim.step", json!({"ticks": 1}))["world_hash"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    pub fn steps(&mut self, n: u32) {
        self.ok("sim.step", json!({ "ticks": n }));
    }

    pub fn set_play(&mut self, entity: &str, component: &str, value: Value) {
        self.ok(
            "component.set",
            json!({"entity": entity, "component": component, "value": value, "world": "play"}),
        );
    }

    pub fn remove_play(&mut self, entity: &str, component: &str) {
        self.ok(
            "component.remove",
            json!({"entity": entity, "component": component, "world": "play"}),
        );
    }

    pub fn patch_play(&mut self, entity: &str, component: &str, value: Value) {
        self.ok(
            "component.patch",
            json!({"entity": entity, "component": component, "patch": value, "world": "play"}),
        );
    }

    pub fn audio(&self) -> AudioShared {
        self.engine.state::<AudioShared>().unwrap().clone()
    }

    /// The left channel of the offline capture.
    pub fn left(&self) -> Vec<f32> {
        self.audio()
            .lock()
            .capture()
            .chunks(2)
            .map(|f| f[0])
            .collect()
    }

    pub fn right(&self) -> Vec<f32> {
        self.audio()
            .lock()
            .capture()
            .chunks(2)
            .map(|f| f[1])
            .collect()
    }

    pub fn audio_hash(&self) -> String {
        self.audio().lock().render_stats().unwrap().hash
    }
}

pub fn at(x: f32, y: f32, z: f32) -> Value {
    json!({"translation": [x, y, z], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]})
}

pub fn close(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!((a - e).abs() < 1e-5, "frame {i}: {a} != {e}");
    }
}
