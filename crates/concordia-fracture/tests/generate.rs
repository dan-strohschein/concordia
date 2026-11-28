//! `fracture.generate` and `fracture.inspect` through the Engine API (M8 8.1c): a recipe in
//! `.breakable.ron` is the source of truth; the pieces are derived data in the DDC.

use concordia_api::{ApiErrorKind, Engine, EngineConfig};
use concordia_app::Project;
use concordia_core::id::SessionId;
use concordia_fracture::Breakable;
use serde_json::{Value, json};

const ACTOR: &str = "agent:tech-artist";
const RECIPE: &str = "breakables/slab.breakable.ron";

struct Rig {
    dir: tempfile::TempDir,
    engine: Engine,
    session: SessionId,
}

impl Rig {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("concordia.toml"),
            "[assets]\n\n[gen]\n\n[fracture]\n",
        )
        .unwrap();
        let project = Project::load(dir.path()).unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.modules = project.modules().unwrap();
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut rig = Self {
            dir,
            engine,
            session,
        };
        // Edits go through a scene's command log (undo), as graph.apply's do.
        rig.ok(
            "scene.new",
            json!({"path": "scenes/test.scene.ron", "name": "Test", "actor": ACTOR}),
        );
        rig
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, concordia_api::ApiError> {
        self.engine.call(self.session, method, params)
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    /// A generated 2 × 1 × 1 box (its glTF splits vertices along the edges) as "asset:<id>#mesh/…".
    fn slab(&mut self) -> String {
        let out = self.ok(
            "gen.run",
            json!({"provider": "mesh", "request": {"Box": {"size": [2.0, 1.0, 1.0]}},
                   "path": "assets/models/slab.glb", "actor": ACTOR}),
        );
        let mesh = out["sub_assets"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|s| s.starts_with("mesh/"))
            .unwrap_or_else(|| panic!("a mesh sub-asset: {out}"))
            .to_owned();
        format!("asset:{}#{mesh}", out["id"].as_str().unwrap())
    }

    fn generate(&mut self, mesh: &str) -> Value {
        self.ok(
            "fracture.generate",
            json!({"mesh": mesh, "pattern": {"Uniform": {"cells": 8}}, "seed": 3,
                   "path": RECIPE, "actor": ACTOR}),
        )
    }

    fn text(&self, rel: &str) -> Option<String> {
        std::fs::read_to_string(self.dir.path().join(rel)).ok()
    }
}

fn total(summary: &Value) -> f64 {
    summary["pieces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["volume"].as_f64().unwrap())
        .sum()
}

#[test]
fn generate_writes_a_canonical_recipe_and_caches_the_pieces() {
    let mut rig = Rig::new();
    let mesh = rig.slab();
    let out = rig.generate(&mesh);
    assert_eq!(out["path"], RECIPE);
    let pieces = out["pieces"].as_array().unwrap();
    assert!((2..=8).contains(&pieces.len()), "{} pieces", pieces.len());
    assert!((total(&out) - 2.0).abs() < 1e-3, "{}", total(&out));
    assert!(
        pieces
            .iter()
            .all(|p| p["interior_triangles"].as_u64().unwrap() > 0)
    );
    // The recipe is canonical text that reads back as what was asked for.
    let text = rig.text(RECIPE).expect("the recipe is written");
    let recipe: Breakable = concordia_scene::read_document(&text).unwrap();
    assert_eq!(recipe.source, mesh);
    assert_eq!(recipe.seed, 3);
    assert_eq!(concordia_scene::write_document(&recipe).unwrap(), text);
    // The pieces are in the DDC, under the key the result names.
    let key = out["cache_key"].as_str().unwrap();
    let inspect = rig.ok("fracture.inspect", json!({"path": RECIPE}));
    assert_eq!(inspect["cached"], true);
    assert_eq!(inspect["cache_key"], key);
    assert_eq!(inspect["pieces"], out["pieces"]);
}

#[test]
fn pieces_rebuild_from_the_recipe_when_the_cache_is_gone() {
    let mut rig = Rig::new();
    let mesh = rig.slab();
    let out = rig.generate(&mesh);
    std::fs::remove_dir_all(rig.dir.path().join(".concordia/ddc")).unwrap();
    let rebuilt = rig.ok("fracture.inspect", json!({"path": RECIPE}));
    assert_eq!(rebuilt["cached"], false, "rebuilt, not read");
    assert_eq!(rebuilt["pieces"], out["pieces"], "the same pieces");
    let again = rig.ok("fracture.inspect", json!({"path": RECIPE}));
    assert_eq!(again["cached"], true, "the rebuild was cached");
}

#[test]
fn generate_is_one_undoable_edit_by_its_actor() {
    let mut rig = Rig::new();
    let mesh = rig.slab();
    rig.generate(&mesh);
    assert!(rig.text(RECIPE).is_some());
    rig.ok("txn.undo", json!({"actor": ACTOR}));
    assert!(rig.text(RECIPE).is_none(), "undo removes the new recipe");
    rig.ok("txn.redo", json!({"actor": ACTOR}));
    assert!(rig.text(RECIPE).is_some(), "redo writes it back");
}

#[test]
fn bad_requests_are_errors_naming_the_field() {
    let mut rig = Rig::new();
    let mesh = rig.slab();
    let base = json!({"mesh": mesh, "pattern": {"Uniform": {"cells": 8}}, "seed": 3,
                      "path": RECIPE, "actor": ACTOR});
    let with = |key: &str, value: Value| {
        let mut p = base.clone();
        match value {
            Value::Null => p.as_object_mut().unwrap().remove(key),
            v => p.as_object_mut().unwrap().insert(key.into(), v),
        };
        p
    };
    let err = rig
        .call("fracture.generate", with("actor", Value::Null))
        .unwrap_err();
    assert_eq!(err.kind, ApiErrorKind::ActorRequired, "{err:?}");
    for (params, pointer) in [
        (
            with("pattern", json!({"Uniform": {"cells": 0}})),
            "/pattern",
        ),
        (with("path", json!("breakables/slab.ron")), "/path"),
        (with("mesh", json!("asset:not-an-asset")), "/mesh"),
        (with("metres_per_tile", json!(-1.0)), "/metres_per_tile"),
    ] {
        let err = rig.call("fracture.generate", params.clone()).unwrap_err();
        assert_eq!(err.kind, ApiErrorKind::InvalidParams, "{params}: {err:?}");
        assert_eq!(
            err.data.pointer.as_deref(),
            Some(pointer),
            "{params}: {err:?}"
        );
    }
    assert!(rig.text(RECIPE).is_none(), "nothing was written");
    let err = rig
        .call(
            "fracture.inspect",
            json!({"path": "breakables/none.breakable.ron"}),
        )
        .unwrap_err();
    assert_eq!(err.kind, ApiErrorKind::InvalidParams, "{err:?}");
}
