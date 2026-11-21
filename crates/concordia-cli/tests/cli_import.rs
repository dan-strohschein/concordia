//! `concordia import` (M1 phase 1.4).

mod common;

use common::{run, stderr, stdout};

#[test]
fn import_writes_sidecars_and_prints_ids() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../concordia-import-gltf/tests/fixtures/two_nodes.gltf");
    std::fs::copy(fixture, dir.path().join("two_nodes.gltf")).unwrap();
    let output = run(
        &[
            "import",
            "--project",
            dir.path().to_str().unwrap(),
            "two_nodes.gltf",
        ],
        dir.path(),
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).starts_with("two_nodes.gltf asset:"),
        "{}",
        stdout(&output)
    );
    assert!(dir.path().join("two_nodes.gltf.import.ron").is_file());
    let missing = run(
        &[
            "import",
            "--project",
            dir.path().to_str().unwrap(),
            "nope.png",
        ],
        dir.path(),
    );
    assert_eq!(missing.status.code(), Some(1), "{}", stderr(&missing));
}

#[test]
fn import_handles_fbx() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fbx/static_quad.fbx");
    std::fs::copy(fixture, dir.path().join("static_quad.fbx")).unwrap();
    let output = run(
        &[
            "import",
            "--project",
            dir.path().to_str().unwrap(),
            "static_quad.fbx",
        ],
        dir.path(),
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).starts_with("static_quad.fbx asset:"),
        "{}",
        stdout(&output)
    );
    assert!(dir.path().join("static_quad.fbx.import.ron").is_file());
}
