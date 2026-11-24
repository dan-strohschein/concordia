//! `[ui]` in a project (M2 phase 2.7b.1): the section adds the module, `[assets]` gains the `font`
//! and `theme` importers, and an imported font and theme reach `ui.inspect`.

use std::path::Path;

use concordia_api::{Engine, EngineConfig};
use concordia_app::Project;
use serde_json::{Value, json};

const ACTOR: &str = "agent:test";

#[test]
fn ui_section_adds_the_module_and_its_importers() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("concordia.toml"), "[assets]\n\n[ui]\n").unwrap();
    let font = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../concordia-ui/fonts/NotoSansMono-Regular.ttf");
    std::fs::create_dir_all(root.join("assets/fonts")).unwrap();
    std::fs::copy(font, root.join("assets/fonts/mono.ttf")).unwrap();
    std::fs::create_dir_all(root.join("ui")).unwrap();
    let project = Project::load(root).unwrap();
    let names: Vec<&str> = project
        .modules()
        .unwrap()
        .iter()
        .map(|m| m.name())
        .collect();
    assert!(names.contains(&"ui"), "{names:?}");
    let without = tempfile::tempdir().unwrap();
    std::fs::write(without.path().join("concordia.toml"), "[assets]\n").unwrap();
    let names: Vec<&str> = Project::load(without.path())
        .unwrap()
        .modules()
        .unwrap()
        .iter()
        .map(|m| m.name())
        .collect();
    assert!(!names.contains(&"ui"), "no [ui], no module");

    let mut config = EngineConfig::new(root);
    config.modules = project.modules().unwrap();
    let mut engine = Engine::new(config).unwrap();
    let s = engine.open_session();
    let mut ok = |method: &str, params: Value| {
        engine
            .call(s, method, params)
            .unwrap_or_else(|e| panic!("{method}: {e:?}"))
    };
    ok(
        "scene.new",
        json!({"path": "scenes/a.scene.ron", "name": "A", "actor": ACTOR}),
    );
    let font = ok(
        "asset.import",
        json!({"path": "assets/fonts/mono.ttf", "actor": ACTOR}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let theme_text = format!(
        "Theme(classes: {{\"label\": (base: (font: Some(\"asset:{font}\"), font_size: Some(20.0)))}})\n"
    );
    std::fs::write(root.join("ui/main.theme.ron"), theme_text).unwrap();
    let theme = ok(
        "asset.import",
        json!({"path": "ui/main.theme.ron", "actor": ACTOR}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let made = ok(
        "entity.spawn",
        json!({"components": {"UiRoot": {"canvas": [400.0, 300.0]}, "UiNode": {}}, "actor": ACTOR}),
    );
    let root_id = made["id"].as_str().unwrap().to_owned();
    let label = ok(
        "entity.spawn",
        json!({"components": {
            "Parent": root_id,
            "UiNode": {},
            "UiText": {"runs": [{"text": "MMMM"}]},
            "UiThemed": {"theme": format!("asset:{theme}"), "class": "label"},
        }, "actor": ACTOR}),
    );
    let r = ok("ui.inspect", json!({"entity": label["id"]}));
    let node = &r["nodes"][0];
    assert_eq!(node["style"]["font"], format!("asset:{font}"));
    assert_eq!(node["style"]["font_size"], 20.0);
    assert_eq!(node["lines"][0]["text"], "MMMM");
    // Noto Sans Mono's M advances 600/1000 em: four at 20 px are 48 px.
    let width = node["lines"][0]["width"].as_f64().unwrap();
    assert!((width - 48.0).abs() < 1e-3, "{r}");
    assert!(r["problems"].as_array().unwrap().is_empty(), "{r}");
}

/// A font only a template names is loaded for the simulation (M3 phase 3.0c): the UI's play
/// assets are a snapshot taken when play starts, and it now covers the template catalog, so a
/// label spawned from `templates/label.template.ron` during play finds its imported Noto Sans
/// Mono. Without it, the simulation's layout warns "UI asset not found" and measures with the
/// fallback font.
#[test]
fn a_font_only_a_template_names_is_loaded_for_the_simulation() {
    use concordia_core::id::{DeterministicIdGen, EntityId, IdSource};
    use tracing_subscriber::layer::SubscriberExt;
    let ring = std::sync::Arc::new(concordia_core::log::LogRing::new(
        std::num::NonZeroUsize::new(256).unwrap(),
    ));
    // Process-wide (nextest runs each test in its own process): `ui.interact` may run on a job
    // thread, which a thread-scoped subscriber wouldn't see.
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(ring.layer()))
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // `ui.interact` runs only with input: the starter map.
    std::fs::write(
        root.join("concordia.toml"),
        "[assets]\n\n[ui]\n\n[input]\nmap = \"input/game.input.ron\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("input")).unwrap();
    std::fs::write(
        root.join("input/game.input.ron"),
        concordia_input::InputMap::starter().to_ron().unwrap(),
    )
    .unwrap();
    let font = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../concordia-ui/fonts/NotoSansMono-Regular.ttf");
    std::fs::create_dir_all(root.join("assets/fonts")).unwrap();
    std::fs::copy(font, root.join("assets/fonts/mono.ttf")).unwrap();
    let project = Project::load(root).unwrap();
    let mut config = EngineConfig::new(root);
    config.modules = project.modules().unwrap();
    let mut engine = Engine::new(config).unwrap();
    let s = engine.open_session();
    let ok = |engine: &mut Engine, method: &str, params: Value| {
        engine
            .call(s, method, params)
            .unwrap_or_else(|e| panic!("{method}: {e:?}"))
    };
    ok(
        &mut engine,
        "scene.new",
        json!({"path": "scenes/a.scene.ron", "name": "A", "actor": ACTOR}),
    );
    let font = ok(
        &mut engine,
        "asset.import",
        json!({"path": "assets/fonts/mono.ttf", "actor": ACTOR}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    std::fs::create_dir_all(root.join("templates")).unwrap();
    std::fs::write(
        root.join("templates/label.template.ron"),
        format!(
            "Template(format: 1, description: \"A label in mono.\", traits: {{}}, components: {{\n  \"UiRoot\": (canvas: (400.0, 300.0)),\n  \"UiNode\": (),\n  \"UiText\": (runs: [(text: \"MMMM\", font: Some(\"asset:{font}\"))]),\n}})\n"
        ),
    )
    .unwrap();
    ok(&mut engine, "sim.step", json!({"ticks": 1}));
    let label = engine
        .template_catalog()
        .unwrap()
        .get("label")
        .unwrap()
        .compile(engine.registry())
        .unwrap();
    let id: EntityId = DeterministicIdGen::new(77).next_id().unwrap();
    label.spawn(engine.play_world_mut().unwrap(), id).unwrap();
    ok(&mut engine, "sim.step", json!({"ticks": 2}));
    let missing: Vec<_> = ring
        .tail(None)
        .into_iter()
        .filter(|r| r.message.contains("UI asset not found"))
        .collect();
    assert!(missing.is_empty(), "{missing:?}");
}
