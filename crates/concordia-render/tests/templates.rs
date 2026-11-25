//! The render module's entity templates (M1 phase 1.13): each one created through `entity.create`
//! shows up in the render extraction as what it claims to be. CPU only.

mod common;

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
        Arc::new(RenderModule::new(Arc::new(common::open_device))),
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

/// A crowd member drawn as a cube from 20 m and not at all from 50 m (2.4b.2): `Representation`
/// is a plain component value in a template.
const WALKER: &str = r#"Template(
    format: 1,
    description: "A crowd member with a cheap far representation.",
    traits: {},
    components: {
        "MeshRenderer": (
            material: "builtin:material/default",
            mesh: "builtin:mesh/sphere",
        ),
        "Representation": (
            hidden_from: 50.0,
            instanced: Some((
                material: "builtin:material/default",
                mesh: "builtin:mesh/cube",
            )),
            instanced_from: 20.0,
        ),
        "Transform": (
            rotation: (0.0, 0.0, 0.0, 1.0),
            scale: (1.0, 1.0, 1.0),
            translation: (30.0, 0.0, 0.0),
        ),
    },
)
"#;

#[test]
fn templates_carry_a_representation() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("templates")).unwrap();
    std::fs::write(dir.path().join("templates/walker.template.ron"), WALKER).unwrap();
    let mut config = EngineConfig::new(dir.path());
    config.modules = vec![
        Arc::new(AssetsModule::new(dir.path())),
        Arc::new(RenderModule::new(Arc::new(common::open_device))),
    ];
    let mut engine = Engine::new(config).unwrap();
    let s = engine.open_session();
    engine
        .call(
            s,
            "scene.new",
            json!({"path": "scenes/t.scene.ron", "name": "T", "actor": "agent:test"}),
        )
        .unwrap();
    let listed = engine.call(s, "entity.templates", json!({})).unwrap();
    let walker = listed["templates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "walker")
        .unwrap_or_else(|| panic!("{listed}"));
    assert!(walker["problem"].is_null(), "{walker}");
    let id = engine
        .call(
            s,
            "entity.create",
            json!({"template": "walker", "actor": "agent:test"}),
        )
        .unwrap()["id"]
        .clone();
    let world = engine.world(WorldSel::edit).unwrap();
    let drawn = RenderWorld::extract(world).represented(concordia_core::math::Vec3::ZERO);
    let draw = drawn
        .draws
        .iter()
        .find(|d| d.entity.to_string() == id.as_str().unwrap())
        .unwrap();
    assert_eq!(
        draw.mesh.to_string(),
        "builtin:mesh/cube",
        "30 m away: instanced"
    );
}
