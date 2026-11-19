//! The render module's entity templates (M1 phase 1.13): each one created through `entity.create`
//! shows up in the render extraction as what it claims to be. CPU only.

use std::sync::Arc;

use concordia_api::{Engine, EngineConfig, EngineModule, WorldSel};
use concordia_assets::AssetsModule;
use concordia_render::{RenderModule, RenderWorld};
use serde_json::json;

fn engine() -> (tempfile::TempDir, Engine, concordia_core::id::SessionId) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = EngineConfig::new(dir.path());
    let modules: Vec<Arc<dyn EngineModule>> = vec![
        Arc::new(AssetsModule::new(dir.path())),
        Arc::new(RenderModule::new(Arc::new(
            concordia_rhi_metal::new_headless,
        ))),
    ];
    config.modules = modules;
    let mut engine = Engine::new(config).unwrap();
    let s = engine.open_session();
    engine
        .call(
            s,
            "scene.new",
            json!({"path": "scenes/t.scene.ron", "name": "T", "actor": "agent:test"}),
        )
        .unwrap();
    (dir, engine, s)
}

#[test]
fn entity_create_spawns_each_render_template_renderable() {
    let (_dir, mut engine, s) = engine();
    let list = engine.call(s, "entity.templates", json!({})).unwrap();
    let names: Vec<String> = list["templates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect();
    let expected = [
        ("Cube", 1, 0, 0),
        ("Sphere", 1, 0, 0),
        ("Plane", 1, 0, 0),
        ("Camera", 0, 0, 1),
        ("Directional Light", 0, 1, 0),
        ("Point Light", 0, 1, 0),
        ("Spot Light", 0, 1, 0),
    ];
    let (mut draws, mut lights, mut cameras) = (0, 0, 0);
    for (name, d, l, c) in expected {
        assert!(
            names.iter().any(|n| n == name),
            "{name} missing from {names:?}"
        );
        engine
            .call(
                s,
                "entity.create",
                json!({"template": name, "actor": "agent:test"}),
            )
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let world = RenderWorld::extract(engine.world(WorldSel::edit).unwrap());
        (draws, lights, cameras) = (draws + d, lights + l, cameras + c);
        assert_eq!(world.draws.len(), draws, "after {name}");
        assert_eq!(
            world.lights.len() + world.punctual.len(),
            lights,
            "after {name}"
        );
        assert_eq!(world.cameras.len(), cameras, "after {name}");
    }
}
