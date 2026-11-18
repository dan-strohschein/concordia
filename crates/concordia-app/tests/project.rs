//! Project manifests and the default engine modules (M1 phase 1.1).

use std::path::Path;

use concordia_app::{AppError, Project};

fn examples() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples"))
}

fn project_with(manifest: &str) -> (tempfile::TempDir, Result<Project, AppError>) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("concordia.toml"), manifest).unwrap();
    let project = Project::load(dir.path());
    (dir, project)
}

#[test]
fn m0_sandbox_manifest_still_loads() {
    let project = Project::load(&examples().join("m0-sandbox")).unwrap();
    assert_eq!(project.manifest.project.name, "m0-sandbox");
    assert_eq!(project.manifest.window.title, "Concordia");
    assert!(project.manifest.input.map.is_none());
}

#[test]
fn m1_example_manifest_has_window_and_input_map() {
    let project = Project::load(&examples().join("m1-physics-stack")).unwrap();
    assert_eq!(project.manifest.window.width.get(), 1280);
    assert_eq!(project.input_map().unwrap().action_names().count(), 5);
}

#[test]
fn project_manifest_rejects_unknown_keys() {
    let (_dir, project) = project_with("[project]\nname = \"x\"\nnmae = \"y\"\n");
    let err = project.unwrap_err();
    assert!(matches!(err, AppError::Manifest { .. }), "{err}");
    assert!(err.to_string().contains("nmae"), "{err}");
}

#[test]
fn missing_manifest_means_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let project = Project::load(dir.path()).unwrap();
    assert_eq!(project.manifest.project.name, "");
    assert_eq!(project.input_map().unwrap().action_names().count(), 0);
}

#[test]
fn input_map_path_must_stay_inside_the_project() {
    let (_dir, project) = project_with("[input]\nmap = \"../elsewhere.input.ron\"\n");
    let err = project.unwrap_err();
    assert!(err.to_string().contains("inside the project"), "{err}");
}

#[test]
fn broken_input_map_is_reported_with_its_path() {
    let (dir, project) = project_with("[input]\nmap = \"bad.input.ron\"\n");
    std::fs::write(dir.path().join("bad.input.ron"), "InputMap(actions: 3)").unwrap();
    let err = project.unwrap().input_map().unwrap_err();
    assert!(err.to_string().contains("bad.input.ron"), "{err}");
}

#[test]
fn m1_example_modules_follow_core_in_manifest_order() {
    let project = Project::load(&examples().join("m1-physics-stack")).unwrap();
    let names: Vec<&str> = project
        .modules()
        .unwrap()
        .iter()
        .map(|m| m.name())
        .collect();
    assert_eq!(names, ["input", "physics", "assets", "render"]);
    assert_eq!(
        project.manifest.physics.as_ref().unwrap().gravity,
        [0.0, -9.81, 0.0]
    );
}

#[test]
fn m1_stack_scene_settles_in_a_headless_engine() {
    use concordia_api::{Engine, EngineConfig, WorldSel};
    use serde_json::json;

    let root = examples().join("m1-physics-stack");
    let project = Project::load(&root).unwrap();
    let mut config = EngineConfig::new(&root);
    config.modules = project.modules().unwrap();
    config.threads = std::num::NonZeroUsize::MIN;
    let mut engine = Engine::new(config).unwrap();
    let session = engine.open_session();
    let main = project.manifest.scene.main.clone().unwrap();
    engine
        .call(session, "scene.open", json!({ "path": main }))
        .unwrap();
    engine
        .call(session, "sim.step", json!({"ticks": 180}))
        .unwrap();
    let play = engine.world(WorldSel::play).unwrap();
    let moving: Vec<f32> = play
        .entity_ids_sorted()
        .into_iter()
        .filter_map(|id| play.get::<concordia_scene::builtin::Velocity>(id))
        .map(|v| v.linear.length())
        .collect();
    assert_eq!(
        moving.len(),
        13,
        "ten pyramid boxes and three dropped boxes are dynamic"
    );
    assert!(moving.iter().all(|s| *s < 0.05), "settled: {moving:?}");
}

#[test]
fn projects_without_an_input_map_have_no_input_module() {
    let project = Project::load(&examples().join("m0-sandbox")).unwrap();
    assert!(project.modules().unwrap().is_empty());
}

#[test]
fn assets_module_imports_gltf_through_the_api() {
    use concordia_api::{Engine, EngineConfig};
    use serde_json::json;

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("concordia.toml"), "[assets]\n").unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../concordia-import-gltf/tests/fixtures/two_nodes.gltf");
    std::fs::copy(fixture, dir.path().join("two_nodes.gltf")).unwrap();
    let project = Project::load(dir.path()).unwrap();
    let mut config = EngineConfig::new(dir.path());
    config.modules = project.modules().unwrap();
    let mut engine = Engine::new(config).unwrap();
    let s = engine.open_session();
    let result = engine
        .call(
            s,
            "asset.import",
            json!({"path": "two_nodes.gltf", "actor": "agent:test"}),
        )
        .unwrap();
    assert!(
        result["sub_assets"].to_string().contains("mesh/Tri/0"),
        "{result}"
    );
    let schema = engine
        .call(s, "schema.get", json!({"name": "MeshRenderer"}))
        .unwrap();
    assert_eq!(schema["schema"]["title"], "MeshRenderer");
}

#[test]
fn m1_stack_scene_capture_shows_the_settled_stack() {
    use base64::Engine as _;
    use concordia_api::{Engine, EngineConfig};
    use serde_json::json;

    if let Err(concordia_rhi::RhiError::NoDevice { reason }) = concordia_rhi_metal::new_headless() {
        assert!(
            !concordia_testkit::gpu_required(),
            "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
        );
        return;
    }
    let root = examples().join("m1-physics-stack");
    let project = Project::load(&root).unwrap();
    let mut config = EngineConfig::new(&root);
    config.modules = project.modules().unwrap();
    let mut engine = Engine::new(config).unwrap();
    let session = engine.open_session();
    let main = project.manifest.scene.main.clone().unwrap();
    engine
        .call(session, "scene.open", json!({ "path": main }))
        .unwrap();
    engine
        .call(session, "sim.step", json!({"ticks": 180}))
        .unwrap();
    let shot = engine
        .call(
            session,
            "capture.screenshot",
            json!({"world": "play", "width": 640, "height": 360}),
        )
        .unwrap();
    assert_eq!(shot["sim_tick"], 180);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(shot["image"]["data_base64"].as_str().unwrap())
        .unwrap();
    if let Some(path) = std::env::var_os("CONCORDIA_KEEP_CAPTURE") {
        std::fs::write(path, &bytes).unwrap();
    }
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut rgba).unwrap();
    let pixels = rgba.as_chunks::<4>().0;
    let covered = pixels.iter().filter(|p| **p != pixels[0]).count() * 100 / pixels.len();
    assert!(
        covered > 30,
        "ground and boxes cover the view, not just the clear color: {covered}%"
    );
    let mut colors: Vec<&[u8; 4]> = pixels.iter().collect();
    colors.sort_unstable();
    colors.dedup();
    assert!(
        colors.len() > 10,
        "lit faces have distinct shades: {} colors",
        colors.len()
    );
}
