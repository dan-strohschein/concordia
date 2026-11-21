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
fn assets_module_imports_fbx_through_the_api() {
    use concordia_api::{Engine, EngineConfig};
    use serde_json::json;

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("concordia.toml"), "[assets]\n").unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fbx/skinned.fbx");
    std::fs::copy(fixture, dir.path().join("skinned.fbx")).unwrap();
    let project = Project::load(dir.path()).unwrap();
    let mut config = EngineConfig::new(dir.path());
    config.modules = project.modules().unwrap();
    let mut engine = Engine::new(config).unwrap();
    let s = engine.open_session();
    let result = engine
        .call(
            s,
            "asset.import",
            json!({"path": "skinned.fbx", "actor": "agent:test"}),
        )
        .unwrap();
    let subs = result["sub_assets"].to_string();
    for sub in ["mesh/Body/0", "skin/0", "material/Skin"] {
        assert!(subs.contains(sub), "{sub}: {result}");
    }
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

#[test]
fn studio_section_enables_the_studio_module() {
    let (_dir, project) = project_with("[studio]\n");
    let project = project.unwrap();
    assert_eq!(project.manifest.studio.as_ref().unwrap().dir, "studio");
    let names: Vec<&str> = project
        .modules()
        .unwrap()
        .iter()
        .map(|m| m.name())
        .collect();
    assert_eq!(names, ["studio"]);
}

#[test]
fn studio_dir_must_stay_inside_the_project() {
    let (_dir, project) = project_with("[studio]\ndir = \"../team\"\n");
    let err = project.unwrap_err();
    assert!(err.to_string().contains("inside the project"), "{err}");
}

#[test]
fn scripts_section_enables_the_scripts_module_last() {
    let (_dir, project) = project_with(
        "[studio]\n\n[scripts]\n[[scripts.module]]\nkind = \"wasm\"\npath = \"scripts/brain.wasm\"\n\n[[scripts.module]]\nkind = \"native\"\npath = \"target/debug/libmover\"\nsource = \"scripts/mover/src\"\n",
    );
    let project = project.unwrap();
    let scripts = project.manifest.scripts.as_ref().unwrap();
    assert_eq!(scripts.module.len(), 2);
    assert_eq!(
        scripts.module[1].source.as_deref(),
        Some("scripts/mover/src")
    );
    let names: Vec<&str> = project
        .modules()
        .unwrap()
        .iter()
        .map(|m| m.name())
        .collect();
    assert_eq!(
        names,
        ["studio", "scripts"],
        "after every module whose types scripts may use"
    );
    let (_dir, empty) = project_with("[scripts]\n");
    assert!(empty.unwrap().manifest.scripts.unwrap().module.is_empty());
}

#[test]
fn engine_config_applies_the_manifests_sim_and_journal_settings() {
    let (dir, project) =
        project_with("[sim]\nrate_hz = 30\nseed = 42\n\n[journal]\nsync = false\n\n[replay]\n");
    let config = project.unwrap().engine_config().unwrap();
    assert_eq!(config.project, dir.path());
    assert_eq!((config.sim_seed, config.sim_rate.hz()), (42, 30));
    assert!(!config.journal.sync);
    let names: Vec<&str> = config.modules.iter().map(|m| m.name()).collect();
    assert_eq!(names, ["replay"]);
    let (_dir, bad) = project_with("[sim]\nrate_hz = 0\n");
    let err = bad.unwrap().engine_config().unwrap_err();
    assert!(err.to_string().contains("rate_hz"), "{err}");
}

#[test]
fn replay_section_enables_the_replay_module_before_scripts() {
    let names = |toml: &str| -> Vec<String> {
        let (_dir, project) = project_with(toml);
        let modules = project.unwrap().modules().unwrap();
        modules.iter().map(|m| m.name().to_owned()).collect()
    };
    assert_eq!(names("[replay]\n"), ["replay"]);
    assert_eq!(names("[scripts]\n\n[replay]\n"), ["replay", "scripts"]);
    assert!(
        names("[scripts]\n").iter().all(|n| n != "replay"),
        "off without [replay]"
    );
    let (_dir, unknown) = project_with("[replay]\ndir = \"x\"\n");
    assert!(
        unknown.unwrap_err().to_string().contains("dir"),
        "no keys yet"
    );
}

#[test]
fn script_paths_must_stay_inside_the_project_and_name_a_kind() {
    let (_dir, project) =
        project_with("[[scripts.module]]\nkind = \"wasm\"\npath = \"../brain.wasm\"\n");
    let err = project.unwrap_err();
    assert!(err.to_string().contains("inside the project"), "{err}");
    let (_dir, project) = project_with("[[scripts.module]]\nkind = \"lua\"\npath = \"x.lua\"\n");
    let err = project.unwrap_err();
    assert!(err.to_string().contains("lua"), "{err}");
    let (_dir, project) =
        project_with("[[scripts.module]]\nkind = \"native\"\npath = \"lib\"\nsource = \"/abs\"\n");
    assert!(
        project
            .unwrap_err()
            .to_string()
            .contains("inside the project")
    );
}

/// The m2-scripting example needs its module built first (`just wasm` writes `build/`).
#[test]
fn m2_scripting_example_spawns_and_runs_its_wanderers() {
    use concordia_api::{Engine, EngineConfig};
    use serde_json::{Value, json};

    let root = examples().join("m2-scripting");
    assert!(
        root.join("build/wanderers.wasm").is_file(),
        "examples/m2-scripting/build/wanderers.wasm is missing (run `just wasm`)"
    );
    let project = Project::load(&root).unwrap();
    let mut config = EngineConfig::new(&root);
    config.modules = project.modules().unwrap();
    config.threads = std::num::NonZeroUsize::MIN;
    let mut engine = Engine::new(config).unwrap();
    let session = engine.open_session();
    let mut call = |method: &str, params: Value| {
        engine
            .call(session, method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    };
    let templates = call("entity.templates", json!({}));
    let wanderer = templates["templates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "wanderer")
        .unwrap_or_else(|| panic!("{templates}"))
        .clone();
    assert_eq!(
        wanderer["traits"],
        json!(["wanderers.Wanderer"]),
        "{wanderer}"
    );
    call("scene.open", json!({"path": "scenes/main.scene.ron"}));
    call("sim.step", json!({"ticks": 120}));
    let page = |cursor: Option<String>| json!({"with": ["wanderers.Position"], "world": "play", "limit": 1000, "cursor": cursor});
    let rows = call("query.run", page(None))["rows"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(rows.len(), 200, "the spawner placed every wanderer");
    let moved = rows
        .iter()
        .filter(|r| r["components"]["wanderers.Position"] != json!({"x": 0.0, "y": 0.0}))
        .count();
    assert_eq!(moved, 200, "every wanderer walked");
}

/// `[render] closure_budget` may lower the platform's closure budget (8 on Metal) but never raise
/// it: values outside 1..=8 are refused when the project loads (ADR-0068).
#[test]
fn project_cannot_raise_the_closure_budget() {
    for bad in ["9", "0", "255"] {
        let (_dir, project) = project_with(&format!("[render]\nclosure_budget = {bad}\n"));
        let err = project.unwrap_err();
        assert!(matches!(err, AppError::Manifest { .. }), "{err}");
        let text = err.to_string();
        assert!(text.contains("[render] closure_budget"), "{text}");
        assert!(text.contains("1..=8"), "{text}");
        assert!(text.contains("only lower"), "{text}");
    }
    let (_dir, project) = project_with("[render]\nclosure_budget = 3\n");
    let project = project.unwrap();
    assert_eq!(
        project.closure_budget().map(|b| b.get()),
        Some(3),
        "a lower budget is kept"
    );
    let (_dir, project) = project_with("[render]\n");
    assert_eq!(
        project.unwrap().closure_budget(),
        None,
        "none: the platform's"
    );
}

#[test]
fn the_projects_closure_budget_reaches_the_renderer() {
    use concordia_api::Engine;
    use serde_json::json;

    if let Err(concordia_rhi::RhiError::NoDevice { reason }) = concordia_rhi_metal::new_headless() {
        assert!(
            !concordia_testkit::gpu_required(),
            "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
        );
        return;
    }
    let (dir, project) = project_with("[render]\nclosure_budget = 2\n");
    let mut engine = Engine::new(project.unwrap().engine_config().unwrap()).unwrap();
    let s = engine.open_session();
    let scene = json!({"path": "scenes/a.scene.ron", "name": "A", "actor": "agent:test"});
    engine.call(s, "scene.new", scene).unwrap();
    engine
        .call(s, "entity.spawn", json!({"actor": "agent:test", "components": {
            "Transform": {"translation": [0.0, 0.0, 3.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
            "Camera": {},
        }}))
        .unwrap();
    engine
        .call(s, "capture.screenshot", json!({"width": 16, "height": 16}))
        .unwrap();
    let frame = engine.call(s, "render.stats", json!({})).unwrap()["frame"].clone();
    assert_eq!(frame["closure_budget"], 2, "{frame}");
    drop(dir);
}
