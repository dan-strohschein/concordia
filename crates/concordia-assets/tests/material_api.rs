//! `material.get` and `material.set` through the Engine API (M2 phase 2.3c, ADR-0068).

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use concordia_api::{Engine, EngineConfig, Topic};
use concordia_assets::{AssetsModule, read_material, write_material};
use concordia_core::id::SessionId;
use concordia_core::log::LogRing;
use serde_json::{Value, json};
use tracing_subscriber::layer::SubscriberExt;

const PATH: &str = "materials/paint.material.ron";

/// Coat over paint, written out of canonical order and with a comment.
const CAR_PAINT: &str = r#"(
    root: "paint", // the layer
    nodes: {
        "paint": Vertical(top: "coat", bottom: "base"),
        "base": Slab((roughness: 0.5, base_color: (0.6, 0.1, 0.05, 1.0))),
        "coat": Slab((base_color: (0.0, 0.0, 0.0, 1.0), roughness: 0.15, transmittance: (1.0, 1.0, 1.0))),
    },
)
"#;

/// M1's flat format.
const M1_TEXT: &str = "(base_color: (0.8, 0.1, 0.1, 1.0), metallic: 0.0, roughness: 0.7)\n";

struct Fixture {
    engine: Engine,
    session: SessionId,
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new(ring: Option<Arc<LogRing>>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.threads = NonZeroUsize::MIN;
        config.log_ring = ring;
        config.modules = vec![Arc::new(AssetsModule::new(dir.path()))];
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        Self {
            engine,
            session,
            dir,
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, concordia_api::ApiError> {
        self.engine.call(self.session, method, params)
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params)
            .unwrap_or_else(|e| panic!("{method}: {e:?}"))
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.dir.path().join(rel)).unwrap()
    }
}

fn material_json(text: &str) -> Value {
    serde_json::to_value(read_material(text).unwrap()).unwrap()
}

#[test]
fn material_get_and_set_roundtrip_canonical_text() {
    let mut fx = Fixture::new(None);
    fx.write(PATH, CAR_PAINT);
    let got = fx.ok("material.get", json!({"path": PATH}));
    assert_eq!(got["material"], material_json(CAR_PAINT));
    assert_eq!(got["migrated"], false);
    assert!(got["id"].is_null(), "not imported yet: {got}");

    let mut edited = got["material"].clone();
    edited["nodes"]["coat"]["Slab"]["roughness"] = json!(0.25);
    let set = fx.ok(
        "material.set",
        json!({"path": PATH, "material": edited, "actor": "agent:tech-artist#1"}),
    );
    assert!(set["id"].is_string(), "imported: {set}");
    assert_eq!(set["written"], true);

    let text = fx.read(PATH);
    let expected = read_material(CAR_PAINT).map(|mut m| {
        if let Some(concordia_assets::MaterialNode::Slab(coat)) = m.nodes.get_mut("coat") {
            coat.roughness = 0.25;
        }
        m
    });
    assert_eq!(
        text,
        write_material(&expected.unwrap()).unwrap(),
        "the file is canonical text"
    );
    let again = fx.ok("material.get", json!({"path": PATH}));
    assert_eq!(again["material"], edited);
    assert_eq!(again["id"], set["id"], "get reports the imported id");

    let same = fx.ok(
        "material.set",
        json!({"path": PATH, "material": edited, "actor": "agent:tech-artist#1"}),
    );
    assert_eq!(same["written"], false, "identical text is not rewritten");
    assert_eq!(same["id"], set["id"], "the id is stable");
}

#[test]
fn material_get_reads_m1_text_as_one_slab_and_says_it_migrated() {
    let mut fx = Fixture::new(None);
    fx.write(PATH, M1_TEXT);
    let got = fx.ok("material.get", json!({"path": PATH}));
    assert_eq!(got["migrated"], true);
    assert_eq!(got["material"], material_json(M1_TEXT));
    assert_eq!(got["material"]["root"], "surface");

    fx.ok(
        "material.set",
        json!({"path": PATH, "material": got["material"], "actor": "agent:t"}),
    );
    assert_eq!(
        fx.read(PATH),
        write_material(&read_material(M1_TEXT).unwrap()).unwrap(),
        "saving writes the layered format"
    );
    assert_eq!(
        fx.ok("material.get", json!({"path": PATH}))["migrated"],
        false
    );
}

#[test]
fn material_set_creates_and_imports_a_new_material() {
    let mut fx = Fixture::new(None);
    let material = material_json(CAR_PAINT);
    let set = fx.ok(
        "material.set",
        json!({"path": "new/coat.material.ron", "material": material, "actor": "agent:t"}),
    );
    let id = set["id"].as_str().unwrap().to_owned();
    let list = fx.ok("asset.list", json!({}));
    let listed = list["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["path"] == "new/coat.material.ron")
        .unwrap_or_else(|| panic!("listed: {list}"));
    assert_eq!(listed["id"], id);
    assert_eq!(listed["importer"], "material");
}

#[test]
fn material_set_refuses_invalid_materials_naming_node_and_field() {
    let mut fx = Fixture::new(None);
    fx.write(PATH, CAR_PAINT);
    let before = fx.read(PATH);
    let refuse = |fx: &mut Fixture, edit: &dyn Fn(&mut Value)| {
        let mut bad = material_json(CAR_PAINT);
        edit(&mut bad);
        fx.call(
            "material.set",
            json!({"path": PATH, "material": bad, "actor": "agent:t"}),
        )
        .unwrap_err()
    };

    // A reflected range: refused while decoding the params.
    let err = refuse(&mut fx, &|m| {
        m["nodes"]["coat"]["Slab"]["roughness"] = json!(1.5)
    });
    assert_eq!(err.code(), -32602, "{err:?}");
    let pointer = err.data.pointer.clone().unwrap_or_default();
    assert_eq!(pointer, "/material/nodes/coat/Slab/roughness", "{err:?}");

    // An element range: refused by the material's own validation, naming node.field.
    let err = refuse(&mut fx, &|m| {
        m["nodes"]["coat"]["Slab"]["base_color"] = json!([1.5, 0.0, 0.0, 1.0]);
    });
    assert_eq!(err.code(), -32602, "{err:?}");
    assert!(
        err.message.contains("coat.base_color[0]"),
        "names node.field: {}",
        err.message
    );
    assert_eq!(err.data.extra["field"], "coat.base_color[0]", "{err:?}");
    assert_eq!(
        err.data.pointer.as_deref(),
        Some("/material/nodes/coat/Slab/base_color/0"),
        "{err:?}"
    );

    // A dangling reference names both nodes.
    let err = refuse(&mut fx, &|m| {
        m["nodes"]["paint"]["Vertical"]["top"] = json!("lacquer")
    });
    assert_eq!(err.code(), -32602, "{err:?}");
    assert!(
        err.message.contains("lacquer") && err.message.contains("paint"),
        "{}",
        err.message
    );
    assert_eq!(err.data.extra["node"], "paint", "{err:?}");
    assert_eq!(fx.read(PATH), before, "a refused material leaves the file");
}

#[test]
fn material_paths_must_be_project_relative_material_files() {
    let mut fx = Fixture::new(None);
    let material = material_json(CAR_PAINT);
    for path in [
        "../outside.material.ron",
        "/abs/x.material.ron",
        "scenes/a.scene.ron",
        "",
    ] {
        let err = fx
            .call(
                "material.set",
                json!({"path": path, "material": material, "actor": "agent:t"}),
            )
            .unwrap_err();
        assert_eq!(err.code(), -32602, "{path}: {err:?}");
        let err = fx.call("material.get", json!({"path": path})).unwrap_err();
        assert_eq!(err.code(), -32602, "{path}: {err:?}");
    }
    let err = fx
        .call("material.get", json!({"path": "missing.material.ron"}))
        .unwrap_err();
    assert_eq!(err.code(), -32602, "{err:?}");
    assert!(err.message.contains("missing.material.ron"), "{err:?}");
}

#[test]
fn material_set_is_attributed_and_not_journaled() {
    let ring = Arc::new(LogRing::new(NonZeroUsize::new(64).unwrap()));
    let subscriber = tracing_subscriber::registry().with(ring.layer());
    tracing::subscriber::with_default(subscriber, || {
        let mut fx = Fixture::new(Some(Arc::clone(&ring)));
        fx.ok(
            "scene.new",
            json!({"path": "scenes/a.scene.ron", "name": "A", "actor": "agent:t"}),
        );
        let history = fx.ok("txn.history", json!({}));
        let set = fx.ok(
            "material.set",
            json!({"path": PATH, "material": material_json(CAR_PAINT), "actor": "agent:tech-artist#2"}),
        );
        assert_eq!(fx.ok("txn.history", json!({})), history, "not journaled");
        let undo = fx
            .call("txn.undo", json!({"actor": "agent:t"}))
            .unwrap_err();
        assert_eq!(undo.kind.name(), "NothingToUndo", "not undoable: {undo:?}");
        assert_eq!(fx.ok("scene.info", json!({}))["dirty"], false);

        let records = fx.ok("log.tail", json!({"level": "info"}))["records"].clone();
        let saved = records
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["message"] == "material saved")
            .unwrap_or_else(|| panic!("a log record: {records}"));
        assert_eq!(saved["fields"]["actor"], "agent:tech-artist#2");
        assert_eq!(saved["fields"]["path"], PATH);
        assert_eq!(saved["fields"]["id"], set["id"]);
    });
}

#[test]
fn material_set_and_asset_import_publish_asset_events() {
    let mut fx = Fixture::new(None);
    let seen: Arc<Mutex<Vec<Value>>> = Arc::default();
    let sink = Arc::clone(&seen);
    fx.engine.set_event_sink(
        fx.session,
        Box::new(move |event| sink.lock().unwrap().push(event)),
    );
    fx.ok("events.subscribe", json!({"topics": ["asset"]}));
    let set = fx.ok(
        "material.set",
        json!({"path": PATH, "material": material_json(CAR_PAINT), "actor": "agent:t"}),
    );
    fx.write("b.material.ron", M1_TEXT);
    let imported = fx.ok(
        "asset.import",
        json!({"path": "b.material.ron", "actor": "agent:u"}),
    );
    let events: Vec<Value> = seen
        .lock()
        .unwrap()
        .iter()
        .map(|e| e["params"].clone())
        .collect();
    assert_eq!(events.len(), 2, "{events:?}");
    assert_eq!(events[0]["topic"], json!(Topic::asset));
    assert_eq!(events[0]["path"], PATH);
    assert_eq!(events[0]["id"], set["id"]);
    assert_eq!(events[0]["actor"], "agent:t");
    assert_eq!(events[1]["path"], "b.material.ron");
    assert_eq!(events[1]["id"], imported["id"]);
    assert_eq!(events[1]["actor"], "agent:u");
}

#[test]
fn material_methods_have_generated_schemas() {
    let fx = Fixture::new(None);
    for name in ["material.get", "material.set"] {
        let method = fx
            .engine
            .methods()
            .get(name)
            .unwrap_or_else(|| panic!("{name} missing"));
        assert!(method.is_agent_tool(), "{name} is offered to agents");
        assert!(!method.mutates, "{name} never changes the scene");
        assert!(method.docs.len() > 20, "{name} is described");
        let params = concordia_api::params_schema(method);
        assert_eq!(params["type"], "object", "{name}");
    }
    let set = concordia_api::params_schema(fx.engine.methods().get("material.set").unwrap());
    assert!(
        set.to_string().contains("MaterialNode"),
        "material.set takes the reflected Material: {set}"
    );
}

#[test]
fn material_is_a_registered_type_for_schema_get() {
    let mut fx = Fixture::new(None);
    let schema = fx.ok("schema.get", json!({"name": "Material"}))["schema"].clone();
    assert_eq!(schema["title"], "Material", "{schema}");
    assert!(schema["$defs"]["Slab"].is_object(), "{schema}");
    let index = fx.ok("schema.get", json!({}))["schema"].clone();
    let components = index["components"].as_array().unwrap();
    assert!(
        components.iter().all(|c| c["name"] != "Material"),
        "a material is not a component"
    );
}
