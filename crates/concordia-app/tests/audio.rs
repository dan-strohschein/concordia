//! `[audio]` in a project (M2 phase 2.7a.2): the section, the bus layout file, the backend for
//! headless and windowed play, clip events that sound, and a WASM guest that plays a cue.
//! Expected values, by hand (48 kHz, 60 Hz: 800 frames a tick; `once` is 100 frames of 0.5):
//! - The fixture character's Idle `breathe` is at frame 11 of 24 fps, 0.458333 s. The player
//!   starts at 0, so tick t covers (t/60, (t+1)/60]: tick 27 (0.45, 0.4667] crosses it, the
//!   event is delivered in tick 28, and `once` sounds on frames 22,400..22,500. Idle loops at
//!   1 s, so the next breathe (1.458333 s) is crossed in tick 87 and sounds from frame 70,400.
//! - The guest chimes on ticks 0, 30 and 60 (every 30); each `PlayCue` is delivered a tick
//!   later, so `once` sounds from frames 800, 24,800 and 48,800.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use concordia_api::{Engine, EngineConfig};
use concordia_app::{Playback, Project};
use concordia_audio::{AudioShared, Backend};
use serde_json::{Value, json};

const ACTOR: &str = "agent:test";

struct Stage {
    dir: tempfile::TempDir,
    engine: Engine,
    session: concordia_core::id::SessionId,
}

fn wav(path: &Path, frames: usize, value: i16) {
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

impl Stage {
    fn new(manifest: &str, files: &[(PathBuf, &str)]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("concordia.toml"), manifest).unwrap();
        for (from, to) in files {
            let to = root.join(to);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::copy(from, &to).unwrap_or_else(|e| panic!("{}: {e}", from.display()));
        }
        wav(&root.join("assets/sfx/short.wav"), 100, 16384);
        let project = Project::load(root).unwrap();
        let mut config = EngineConfig::new(root);
        config.deterministic_ids = Some(13);
        config.modules = project.modules().unwrap();
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut stage = Self {
            dir,
            engine,
            session,
        };
        stage.ok(
            "scene.new",
            json!({"path": "scenes/a.scene.ron", "name": "A", "actor": ACTOR}),
        );
        stage
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.engine
            .call(self.session, method, params)
            .unwrap_or_else(|e| panic!("{method}: {e:?}"))
    }

    fn import(&mut self, path: &str) -> String {
        let id = self.ok("asset.import", json!({"path": path, "actor": ACTOR}))["id"]
            .as_str()
            .unwrap()
            .to_owned();
        format!("asset:{id}")
    }

    /// `once`: the 100-frame sound, unpositioned, on SFX.
    fn once(&mut self) -> String {
        let short = self.import("assets/sfx/short.wav");
        let path = "assets/sfx/once.cue.ron";
        std::fs::write(
            self.dir.path().join(path),
            format!("(sounds: [(sound: \"{short}\")])\n"),
        )
        .unwrap();
        self.import(path)
    }

    fn left(&self) -> Vec<f32> {
        let shared = self.engine.state::<AudioShared>().unwrap().clone();
        let capture = shared.lock().capture();
        capture.chunks(2).map(|f| f[0]).collect()
    }
}

/// The frames that sound (non-zero), as [start, end) runs.
fn runs(samples: &[f32]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (i, s) in samples.iter().enumerate() {
        if *s == 0.0 {
            continue;
        }
        match out.last_mut() {
            Some((_, end)) if *end == i => *end = i + 1,
            _ => out.push((i, i + 1)),
        }
    }
    out
}

fn character(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/character")
        .join(name)
}

#[test]
fn anim_event_cues_play_on_clip_events() {
    let mut stage = Stage::new(
        "[assets]\n\n[anim]\n\n[audio]\n",
        &[
            (character("character.fbx"), "assets/models/character.fbx"),
            (
                character("character.fbx.import.ron"),
                "assets/models/character.fbx.import.ron",
            ),
        ],
    );
    let once = stage.once();
    let model = stage.import("assets/models/character.fbx");
    let made = stage.ok(
        "prefab.instantiate",
        json!({"asset": model, "actor": ACTOR}),
    );
    let body = made["entities"]
        .as_array()
        .unwrap()
        .clone()
        .into_iter()
        .find(|e| {
            let got = stage.ok("entity.get", json!({"id": e}))["components"].clone();
            got.get("Skinned").is_some()
                && got.get("Parent").is_none_or(|p| {
                    let parent = stage.ok("entity.get", json!({"id": p}))["components"].clone();
                    parent.get("Skinned").is_none()
                })
        })
        .expect("a skinned body");
    let idle = format!("{model}#clip/Character|Idle");
    stage.ok(
        "component.set",
        json!({"entity": body, "component": "AnimPlayer", "value": {"clip": idle}, "actor": ACTOR}),
    );
    stage.ok(
        "component.set",
        json!({"entity": body, "component": "AnimEventCues", "value": {"cues": {"breathe": once, "never": once}}, "actor": ACTOR}),
    );
    stage.ok("sim.step", json!({"ticks": 90}));
    let left = stage.left();
    assert_eq!(left.len(), 90 * 800);
    assert_eq!(runs(&left), [(22_400, 22_500), (70_400, 70_500)]);
}

#[test]
fn scripted_processor_plays_a_cue() {
    let guest = concordia_tests::wasm_fixture_path("audio_guest").unwrap_or_else(|e| panic!("{e}"));
    let mut stage = Stage::new(
        "[assets]\n\n[audio]\n\n[scripts]\n[[scripts.module]]\nkind = \"wasm\"\npath = \"scripts/audio_guest.wasm\"\n",
        &[(guest, "scripts/audio_guest.wasm")],
    );
    let once = stage.once();
    stage.ok(
        "entity.spawn",
        json!({"components": {"aud.Chime": {"cue": once, "every": 30}}, "actor": ACTOR}),
    );
    stage.ok("sim.step", json!({"ticks": 70}));
    assert_eq!(
        runs(&stage.left()),
        [(800, 900), (24_800, 24_900), (48_800, 48_900)]
    );
}

fn project(
    manifest: &str,
    files: &[(&str, &str)],
) -> (tempfile::TempDir, Result<Project, concordia_app::AppError>) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("concordia.toml"), manifest).unwrap();
    for (path, text) in files {
        let to = dir.path().join(path);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(to, text).unwrap();
    }
    let project = Project::load(dir.path());
    (dir, project)
}

#[test]
fn buses_file_loads_and_bad_layouts_are_descriptive() {
    let layout = "(buses: [(name: \"SFX\", gain_db: -3.0), (name: \"Steps\", send: Some(\"SFX\")), (name: \"Music\", gain_db: -6.0)])\n";
    let (_dir, p) = project(
        "[audio]\nbuses = \"audio/main.buses.ron\"\nvoices = 64\n",
        &[("audio/main.buses.ron", layout)],
    );
    let settings = p
        .unwrap()
        .audio_settings(Playback::Headless)
        .unwrap()
        .unwrap();
    assert_eq!(settings.voices.get(), 64);
    let names: Vec<_> = settings
        .buses
        .buses
        .iter()
        .map(|b| b.name.as_str())
        .collect();
    assert_eq!(names, ["SFX", "Steps", "Music"]);
    let cycle = "(buses: [(name: \"A\", send: Some(\"B\")), (name: \"B\", send: Some(\"A\"))])\n";
    for (manifest, files, needle) in [
        (
            "[audio]\nbuses = \"audio/main.buses.ron\"\n",
            vec![("audio/main.buses.ron", cycle)],
            "A -> B -> A",
        ),
        (
            "[audio]\nbuses = \"audio/gone.buses.ron\"\n",
            vec![],
            "audio/gone.buses.ron",
        ),
        (
            "[audio]\nbuses = \"../out.buses.ron\"\n",
            vec![],
            "[audio] buses",
        ),
        ("[audio]\nvoices = 0\n", vec![], "[audio] voices"),
        ("[audio]\nvoices = 300\n", vec![], "[audio] voices"),
        ("[audio]\nbackend = \"loud\"\n", vec![], "backend"),
    ] {
        let (_dir, p) = project(manifest, &files);
        let e = p
            .and_then(|p| p.audio_settings(Playback::Headless).map(|_| ()))
            .unwrap_err()
            .to_string();
        assert!(e.contains(needle), "{manifest}: {e}");
    }
}

#[test]
fn the_backend_follows_the_section_and_the_playback() {
    let backend = |manifest: &str, playback: Playback| {
        let (_dir, p) = project(manifest, &[]);
        match p
            .unwrap()
            .audio_settings(playback)
            .unwrap()
            .unwrap()
            .backend
        {
            Backend::Offline => "offline",
            Backend::Device(_) => "device",
            Backend::None => "none",
        }
    };
    assert_eq!(backend("[audio]\n", Playback::Headless), "offline");
    assert_eq!(backend("[audio]\n", Playback::Windowed), "device");
    assert_eq!(
        backend("[audio]\nbackend = \"offline\"\n", Playback::Windowed),
        "offline"
    );
    assert_eq!(
        backend("[audio]\nbackend = \"none\"\n", Playback::Windowed),
        "none"
    );
    let (_dir, p) = project("[assets]\n", &[]);
    assert!(
        p.unwrap()
            .audio_settings(Playback::Windowed)
            .unwrap()
            .is_none(),
        "no [audio], no audio"
    );
}
