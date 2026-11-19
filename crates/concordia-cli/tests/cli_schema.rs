//! `concordia schema` (M0 phase 0.7).

mod common;

use common::{run, stderr, stdout};
use concordia_api::{Engine, EngineConfig, methods, params_schema};
use serde_json::{Value, json};

#[test]
fn concordia_schema_index_matches_registry() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::new(EngineConfig::new(dir.path())).unwrap();
    let session = engine.open_session();
    let expected = engine.call(session, "schema.get", json!({})).unwrap()["schema"].clone();

    let index: Value = serde_json::from_str(&stdout(&run(&["schema"], dir.path()))).unwrap();
    assert_eq!(index, expected);
    let transform: Value =
        serde_json::from_str(&stdout(&run(&["schema", "Transform"], dir.path()))).unwrap();
    assert_eq!(transform["title"], "Transform");
    let spawn: Value = serde_json::from_str(&stdout(&run(
        &["schema", "--method", "entity.spawn"],
        dir.path(),
    )))
    .unwrap();
    assert_eq!(spawn, params_schema(methods().get("entity.spawn").unwrap()));
    let missing = run(&["schema", "Transfrom"], dir.path());
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("Transform"));
}

#[test]
fn concordia_schema_out_writes_one_sorted_file_per_type() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("schemas");
    let write = || {
        assert!(
            run(&["schema", "--out", out.to_str().unwrap()], dir.path())
                .status
                .success()
        )
    };
    write();
    let mut files: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert!(
        files.contains(&"Transform.schema.json".to_owned()),
        "{files:?}"
    );
    let first = std::fs::read_to_string(out.join("Transform.schema.json")).unwrap();
    write();
    assert_eq!(
        std::fs::read_to_string(out.join("Transform.schema.json")).unwrap(),
        first,
        "deterministic"
    );
}

#[test]
fn schema_out_with_project_includes_module_types_and_method_schemas() {
    let dir = tempfile::tempdir().unwrap();
    let project = format!(
        "{}/../../examples/m1-physics-stack",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = run(
        &[
            "schema",
            "--project",
            &project,
            "--out",
            dir.path().to_str().unwrap(),
        ],
        dir.path(),
    );
    assert!(output.status.success(), "{}", stderr(&output));
    for file in [
        "Camera.schema.json",
        "MeshRenderer.schema.json",
        "RigidBody.schema.json",
        "methods/capture.screenshot.params.schema.json",
        "methods/capture.screenshot.result.schema.json",
        "methods/entity.spawn.params.schema.json",
    ] {
        assert!(dir.path().join(file).is_file(), "{file} written");
    }
    let result: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            dir.path()
                .join("methods/capture.screenshot.result.schema.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(result["title"], "CaptureScreenshotResult");
}
