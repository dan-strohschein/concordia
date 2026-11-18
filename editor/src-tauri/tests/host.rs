//! The editor's engine host (M1 phase 1.9c): an in-process engine with a token-protected API.

use concordia_editor::EngineHost;

fn example_copy() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let src =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/m1-physics-stack");
    copy_dir(&src, dir.path());
    dir
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn api_server_rejects_connections_without_token() {
    let dir = example_copy();
    let host = EngineHost::start(dir.path()).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let bare = host.base_url().to_owned();
        assert!(
            tokio_tungstenite::connect_async(bare).await.is_err(),
            "no token, no connection"
        );
        let wrong = format!("{}/?token=not-the-token", host.base_url());
        assert!(
            tokio_tungstenite::connect_async(wrong).await.is_err(),
            "wrong token, no connection"
        );
        assert!(
            tokio_tungstenite::connect_async(host.url()).await.is_ok(),
            "the injected URL works"
        );
    });
}

#[test]
fn host_opens_the_main_scene_and_writes_the_connection_file() {
    let dir = example_copy();
    let host = EngineHost::start(dir.path()).unwrap();
    let info = host.call("scene.info", serde_json::json!({})).unwrap();
    assert_eq!(info["path"], "scenes/stack.scene.ron");
    assert!(info["entity_count"].as_u64().unwrap() > 10);
    let file: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".concordia/headless.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        file["token"].as_str().unwrap().len(),
        32,
        "agents can attach to the editor's engine"
    );
}

#[test]
fn bad_projects_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let err = EngineHost::start(dir.path()).unwrap_err();
    assert!(err.to_string().contains("concordia.toml"), "{err}");
}
