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

#[test]
fn import_handles_sounds_and_themes() {
    // The engine imports sounds (`sound`) and themes (`theme`) through `[assets]`; the CLI must
    // register the same importers (M2 phase 2.10b): a 1 s Ogg tone and a one-class theme.
    let dir = tempfile::tempdir().unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/audio/tone-440.ogg");
    std::fs::copy(fixture, dir.path().join("tone.ogg")).unwrap();
    std::fs::write(
        dir.path().join("hud.theme.ron"),
        "Theme(classes: {\"label\": (base: (font_size: Some(18.0)))})\n",
    )
    .unwrap();
    let output = run(
        &[
            "import",
            "--project",
            dir.path().to_str().unwrap(),
            "tone.ogg",
            "hud.theme.ron",
        ],
        dir.path(),
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.starts_with("tone.ogg asset:"), "{text}");
    assert!(text.contains("\nhud.theme.ron asset:"), "{text}");
    assert!(dir.path().join("tone.ogg.import.ron").is_file());
    assert!(dir.path().join("hud.theme.ron.import.ron").is_file());
}
