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
