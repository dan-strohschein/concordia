//! `[gen]` in a project (M3 phase 3.7a): `gen.list` and `gen.run` through the engine, with the
//! output imported and its provenance in the sidecar.

use std::path::Path;

use concordia_api::{ApiErrorKind, Engine, EngineConfig};
use concordia_app::Project;
use concordia_assets::{AssetRef, Sidecar};
use concordia_core::id::SessionId;
use serde_json::{Value, json};

const ACTOR: &str = "agent:modeler";
const PILLAR: &str = "assets/models/pillar.glb";

struct Rig {
    dir: tempfile::TempDir,
    engine: Engine,
    session: SessionId,
}

impl Rig {
    fn new() -> Self {
        Self::with_ring(None)
    }

    /// With a log ring that `log.tail` reads (and a process-wide subscriber feeding it; nextest
    /// runs each test in its own process).
    fn logged() -> Self {
        use tracing_subscriber::layer::SubscriberExt;
        let ring = std::sync::Arc::new(concordia_core::log::LogRing::new(
            std::num::NonZeroUsize::new(256).unwrap(),
        ));
        tracing::subscriber::set_global_default(tracing_subscriber::registry().with(ring.layer()))
            .unwrap();
        Self::with_ring(Some(ring))
    }

    fn with_ring(ring: Option<std::sync::Arc<concordia_core::log::LogRing>>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("concordia.toml"), "[assets]\n\n[gen]\n").unwrap();
        let project = Project::load(dir.path()).unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.log_ring = ring;
        config.modules = project.modules().unwrap();
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        Self {
            dir,
            engine,
            session,
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, concordia_api::ApiError> {
        self.engine.call(self.session, method, params)
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    fn bytes(&self, rel: &str) -> Vec<u8> {
        std::fs::read(self.dir.path().join(rel)).unwrap()
    }
}

fn pillar() -> Value {
    json!({"provider": "mesh", "request": {"Cylinder": {"radius": 0.25, "height": 3.0, "segments": 12}},
           "seed": 1, "path": PILLAR, "actor": ACTOR})
}

/// By default only the procedural providers are listed (no model plugins), each with its kind,
/// version, file extensions and request schema.
#[test]
fn registry_lists_procedural_providers_by_default_and_no_model_providers() {
    let mut rig = Rig::new();
    let list = rig.ok("gen.list", json!({}));
    let providers = list["providers"].as_array().unwrap();
    let ids: Vec<&str> = providers
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["mesh"], "{list}");
    let mesh = &providers[0];
    assert_eq!(
        (&mesh["kind"], &mesh["extensions"]),
        (&json!("mesh"), &json!(["glb"]))
    );
    assert!(mesh["request"].is_object(), "a JSON Schema: {mesh}");
    assert_eq!(mesh["procedural"], true);
}

/// `gen.run` writes the file, imports it (an asset id and a mesh sub-asset), and records how it
/// was made in the sidecar. A 12-sided cylinder has 12 · 4 = 48 triangles (sides, two caps
/// fanned around a centre each), which the imported mesh keeps.
#[test]
fn gen_run_writes_output_and_provenance_sidecar() {
    let mut rig = Rig::new();
    let out = rig.ok("gen.run", pillar());
    assert_eq!(out["path"], PILLAR);
    assert_eq!(out["info"]["triangles"], 48, "{out}");
    let sidecar = Sidecar::read(&Sidecar::path_for(&rig.dir.path().join(PILLAR))).unwrap();
    let p = sidecar.provenance.expect("provenance");
    assert_eq!((p.provider.as_str(), p.seed), ("mesh", 1));
    assert_eq!(
        p.request,
        json!({"Cylinder": {"radius": 0.25, "height": 3.0, "segments": 12}})
    );
    assert_eq!(out["inputs_hash"], p.inputs_hash.as_str());
    let id = out["id"].as_str().unwrap();
    let mesh = out["sub_assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s.as_str().is_some_and(|s| s.starts_with("mesh/")))
        .unwrap_or_else(|| panic!("a mesh sub-asset: {out}"))
        .as_str()
        .unwrap()
        .to_owned();
    let reference: AssetRef = format!("asset:{id}#{mesh}").parse().unwrap();
    let db = rig.engine.state_mut::<concordia_assets::AssetDb>().unwrap();
    let imported = db.load_mesh(&reference).unwrap();
    assert_eq!(
        imported.indices.len() / 3,
        48,
        "the GLB re-imports through the glTF importer"
    );
}

/// Running the provenance again reproduces the file byte for byte; another seed of a seeded
/// shape does not.
#[test]
fn regenerating_from_provenance_reproduces_identical_bytes() {
    let mut rig = Rig::new();
    rig.ok("gen.run", pillar());
    let first = rig.bytes(PILLAR);
    let p = Sidecar::read(&Sidecar::path_for(&rig.dir.path().join(PILLAR)))
        .unwrap()
        .provenance
        .unwrap();
    std::fs::remove_file(rig.dir.path().join(PILLAR)).unwrap();
    rig.ok(
        "gen.run",
        json!({"provider": p.provider, "request": p.request, "seed": p.seed, "path": PILLAR, "actor": ACTOR}),
    );
    assert_eq!(rig.bytes(PILLAR), first);
    let ground = |seed: u64| {
        json!({"provider": "mesh", "request": {"Heightfield": {"size": [8.0, 8.0], "cells": [8, 8], "amplitude": 1.0, "frequency": 1.0}},
               "seed": seed, "path": "assets/models/ground.glb", "actor": ACTOR})
    };
    rig.ok("gen.run", ground(1));
    let one = rig.bytes("assets/models/ground.glb");
    rig.ok("gen.run", ground(2));
    assert_ne!(rig.bytes("assets/models/ground.glb"), one);
}

/// Generating writes a file, so it needs an actor and is logged with it; a wrong extension, an
/// unknown provider or a bad request is refused, naming the problem.
#[test]
fn gen_run_requires_actor_and_is_logged() {
    let mut rig = Rig::logged();
    let mut no_actor = pillar();
    no_actor.as_object_mut().unwrap().remove("actor");
    let err = rig.call("gen.run", no_actor).unwrap_err();
    assert_eq!(err.kind, ApiErrorKind::InvalidParams, "{err:?}");
    assert!(err.message.contains("actor"), "{err:?}");
    let mut wrong = pillar();
    wrong["path"] = json!("assets/models/pillar.png");
    let err = rig.call("gen.run", wrong).unwrap_err();
    assert!(err.message.contains("glb"), "{err:?}");
    let mut unknown = pillar();
    unknown["provider"] = json!("dream");
    let err = rig.call("gen.run", unknown).unwrap_err();
    assert!(err.message.contains("mesh"), "lists the providers: {err:?}");
    let mut bad = pillar();
    bad["request"] = json!({"Cylinder": {"radius": -1.0, "height": 3.0, "segments": 12}});
    let err = rig.call("gen.run", bad).unwrap_err();
    assert!(err.message.contains("radius"), "{err:?}");
    assert!(
        !Path::new(&rig.dir.path().join(PILLAR)).exists(),
        "nothing written"
    );
    rig.ok("gen.run", pillar());
    let logs = rig.ok("log.tail", json!({}))["records"].clone();
    let generated = logs
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["message"] == "content generated")
        .unwrap_or_else(|| panic!("{logs}"));
    assert_eq!(generated["fields"]["actor"], ACTOR, "{generated}");
}
