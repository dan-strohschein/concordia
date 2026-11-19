//! `prefab.instantiate`: an imported model becomes entities in one undo step (M1 phase 1.11).

use std::path::Path;

use concordia_api::{Engine, EngineConfig};
use concordia_app::Project;
use concordia_core::id::SessionId;
use serde_json::{Value, json};

const ACTOR: &str = "agent:test";

struct Harness {
    engine: Engine,
    session: SessionId,
    _dir: tempfile::TempDir,
}

impl Harness {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("concordia.toml"),
            "[project]\nname = \"p\"\n\n[assets]\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("models")).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../concordia-import-gltf/tests/fixtures/two_nodes.gltf");
        std::fs::copy(fixture, root.join("models/two_nodes.gltf")).unwrap();
        let project = Project::load(root).unwrap();
        let mut config = EngineConfig::new(root);
        config.modules = project.modules().unwrap();
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut h = Self {
            engine,
            session,
            _dir: dir,
        };
        h.ok(
            "scene.new",
            json!({"path": "scenes/a.scene.ron", "name": "A", "actor": ACTOR}),
        );
        h
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, concordia_api::ApiError> {
        self.engine.call(self.session, method, params)
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params)
            .unwrap_or_else(|e| panic!("{method}: {e:?}"))
    }

    fn count(&mut self) -> u64 {
        self.ok("scene.info", json!({}))["entity_count"]
            .as_u64()
            .unwrap()
    }

    fn component(&mut self, id: &Value, name: &str) -> Value {
        self.ok("component.get", json!({"entity": id, "component": name}))["value"].clone()
    }

    fn import(&mut self) -> String {
        let id = self.ok(
            "asset.import",
            json!({"path": "models/two_nodes.gltf", "actor": ACTOR}),
        )["id"]
            .as_str()
            .unwrap()
            .to_owned();
        format!("asset:{id}")
    }
}

#[test]
fn prefab_instantiate_spawns_the_hierarchy_as_one_undo_step() {
    let mut h = Harness::new();
    let asset = h.import();
    let holder = h.ok(
        "entity.spawn",
        json!({"actor": ACTOR, "components": {"Name": "Holder"}}),
    )["id"]
        .clone();
    let before = h.count();
    let placed = h.ok(
        "prefab.instantiate",
        json!({"asset": asset, "parent": holder, "translation": [0.0, 0.0, 5.0], "actor": ACTOR}),
    );
    let root = placed["root"].clone();
    assert_eq!(placed["entities"].as_array().unwrap().len(), 3, "{placed}");
    assert_eq!(h.count(), before + 3);
    assert_eq!(h.component(&root, "Name"), "two_nodes");
    assert_eq!(h.component(&root, "Parent"), holder);
    assert_eq!(
        h.component(&root, "Transform")["translation"],
        json!([0.0, 0.0, 5.0])
    );
    let children = h.ok(
        "query.run",
        json!({"with": ["Parent", "MeshRenderer"], "where": [{"path": "Parent", "op": "eq", "value": root}]}),
    );
    assert_eq!(children["rows"].as_array().unwrap().len(), 2, "{children}");

    let again = h.ok(
        "prefab.instantiate",
        json!({"asset": asset, "actor": ACTOR}),
    );
    assert_ne!(again["root"], root, "each instance gets fresh ids");
    assert_eq!(h.count(), before + 6);
    h.ok("txn.undo", json!({"actor": ACTOR}));
    assert_eq!(
        h.count(),
        before + 3,
        "one undo removes the whole second instance"
    );
}

#[test]
fn prefab_instantiate_joins_an_open_transaction() {
    let mut h = Harness::new();
    let asset = h.import();
    let before = h.count();
    let txn = h.ok("txn.begin", json!({"label": "Place", "actor": ACTOR}))["txn"].clone();
    h.ok(
        "prefab.instantiate",
        json!({"asset": asset, "actor": ACTOR}),
    );
    assert_eq!(h.count(), before + 3);
    h.ok("txn.abort", json!({"txn": txn, "actor": ACTOR}));
    assert_eq!(
        h.count(),
        before,
        "aborting the caller's transaction removes the instance"
    );
}

#[test]
fn prefab_instantiate_rejects_unknown_assets_and_parents() {
    let mut h = Harness::new();
    let asset = h.import();
    let err = h
        .call(
            "prefab.instantiate",
            json!({"asset": "asset:0190f5c4-0000-7000-8000-000000000000", "actor": ACTOR}),
        )
        .unwrap_err();
    assert_eq!(err.code(), -32602, "{err:?}");
    let before = h.count();
    let err = h
        .call("prefab.instantiate", json!({"asset": asset, "parent": "0190f5c4-0000-7000-8000-000000000001", "actor": ACTOR}))
        .unwrap_err();
    assert!(err.to_string().contains("parent"), "{err:?}");
    assert_eq!(h.count(), before, "a failed instance leaves nothing behind");
}
