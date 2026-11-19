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

#[test]
fn call_each_returns_results_in_order() {
    let dir = example_copy();
    let host = EngineHost::start(dir.path()).unwrap();
    let names = ["a", "b", "c"];
    let results = host
        .call_each(
            "entity.spawn",
            names
                .iter()
                .map(|n| serde_json::json!({"components": {"Name": n}})),
        )
        .unwrap();
    let got: Vec<_> = results
        .iter()
        .map(|r| {
            host.call("entity.get", serde_json::json!({"id": r["id"]}))
                .unwrap()["components"]["Name"]
                .clone()
        })
        .collect();
    assert_eq!(got, names.map(serde_json::Value::from));
}

#[test]
fn bench_scene_adds_named_entities_in_one_undo_step() {
    let dir = example_copy();
    let host = EngineHost::start(dir.path()).unwrap();
    let before = host.call("scene.info", serde_json::json!({})).unwrap()["entity_count"]
        .as_u64()
        .unwrap();
    concordia_editor::bench::add_entities(&host, 250).unwrap();
    let info = host.call("scene.info", serde_json::json!({})).unwrap();
    assert_eq!(info["entity_count"].as_u64().unwrap(), before + 250);
    host.call("txn.undo", serde_json::json!({})).unwrap();
    let undone = host.call("scene.info", serde_json::json!({})).unwrap();
    assert_eq!(
        undone["entity_count"].as_u64().unwrap(),
        before,
        "one transaction"
    );
}

#[test]
fn bench_orbit_views_circle_the_target_at_constant_distance() {
    let views: Vec<_> = concordia_editor::bench::orbit_views(90).collect();
    assert_eq!(views.len(), 90);
    let distance = |v: &serde_json::Value| {
        let (e, t) = (&v["eye"], &v["target"]);
        (0..3)
            .map(|i| (e[i].as_f64().unwrap() - t[i].as_f64().unwrap()).powi(2))
            .sum::<f64>()
            .sqrt()
    };
    let d0 = distance(&views[0]);
    assert!(views.iter().all(|v| (distance(v) - d0).abs() < 1e-6));
    assert_ne!(
        views[0]["eye"], views[1]["eye"],
        "the eye moves every frame"
    );
}

#[test]
fn host_serves_log_tail_from_the_given_log_ring() {
    static LOG: std::sync::OnceLock<concordia_core::log::LogHandle> = std::sync::OnceLock::new();
    let log = LOG.get_or_init(|| {
        concordia_core::log::init(&concordia_core::log::LogConfig::from_env()).unwrap()
    });
    let dir = example_copy();
    let host = EngineHost::start_with_log(dir.path(), Some(log.ring().clone())).unwrap();
    let session = host.session().unwrap();
    let txn = session
        .call("txn.begin", serde_json::json!({"label": "host-log-test"}))
        .unwrap();
    session
        .call(
            "entity.spawn",
            serde_json::json!({"components": {"Name": "Logged"}}),
        )
        .unwrap();
    session
        .call("txn.commit", serde_json::json!({"txn": txn["txn"]}))
        .unwrap();
    let tail = host
        .call(
            "log.tail",
            serde_json::json!({"level": "info", "limit": 1000}),
        )
        .unwrap();
    let ours = tail["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["message"] == "edit committed" && r["fields"]["label"] == "host-log-test");
    assert_eq!(
        ours.map(|r| r["fields"]["actor"].clone()),
        Some(serde_json::json!("tool:editor-shell")),
        "{tail}"
    );
}

#[test]
fn bench_reads_this_process_resident_memory() {
    let mb = concordia_editor::bench::rss_mb(std::process::id()).expect("ps reports our RSS");
    assert!(mb > 1.0 && mb < 100_000.0, "{mb} MB");
}

#[test]
fn session_as_attributes_edits_to_the_given_actor() {
    let dir = example_copy();
    let host = EngineHost::start(dir.path()).unwrap();
    let human = host.session_as("human:local").unwrap();
    let txn = human
        .call("txn.begin", serde_json::json!({"label": "by hand"}))
        .unwrap();
    human
        .call(
            "entity.spawn",
            serde_json::json!({"components": {"Name": "Mine"}}),
        )
        .unwrap();
    human
        .call("txn.commit", serde_json::json!({"txn": txn["txn"]}))
        .unwrap();
    let history = human.call("txn.history", serde_json::json!({})).unwrap();
    let text = history.to_string();
    assert!(
        text.contains("by hand") && text.contains("human:local"),
        "{text}"
    );
}

/// Another process's engine for `project` (here: as if Claude Code's `concordia mcp` hosted it).
fn mcp_host(project: &std::path::Path) -> (tokio::runtime::Runtime, concordia_app::SharedEngine) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let mut config = concordia_api::EngineConfig::new(project);
    config.modules = concordia_app::Project::load(project)
        .unwrap()
        .modules()
        .unwrap();
    let engine = runtime
        .block_on(concordia_app::SharedEngine::attach_or_host(
            config,
            concordia_api::transport::connection::HostKind::Mcp,
        ))
        .unwrap();
    assert!(engine.is_host());
    (runtime, engine)
}

#[test]
fn editor_attaches_when_an_engine_is_live() {
    let dir = example_copy();
    let (runtime, engine) = mcp_host(dir.path());
    let host = EngineHost::start(dir.path()).unwrap();
    assert!(!host.is_host(), "attached, so no native viewport");
    assert_eq!(host.url(), engine.info().connect_url());
    let info = host.call("scene.info", serde_json::json!({})).unwrap();
    assert_eq!(
        info["path"], "scenes/stack.scene.ron",
        "the editor opens the main scene when the shared engine has none"
    );
    // The edit landed in the MCP server's engine.
    let link = engine.link();
    let seen = runtime.block_on(async {
        let session = link.open_session().await.unwrap();
        link.call(session, "scene.info", serde_json::json!({}))
            .await
            .unwrap()
    });
    assert_eq!(seen["path"], "scenes/stack.scene.ron");
    assert_eq!(
        concordia_api::transport::connection::ConnectionInfo::read(dir.path())
            .unwrap()
            .unwrap()
            .host,
        concordia_api::transport::connection::HostKind::Mcp,
        "the editor publishes nothing when attached"
    );
}

#[test]
fn editor_hosts_when_the_connection_file_is_stale() {
    let dir = example_copy();
    let mut exited = std::process::Command::new("true").spawn().unwrap();
    let pid = exited.id();
    exited.wait().unwrap();
    concordia_api::transport::connection::ConnectionInfo {
        url: "ws://127.0.0.1:9/".into(),
        token: "stale".into(),
        pid,
        version: concordia_core::version().into(),
        host: concordia_api::transport::connection::HostKind::Mcp,
    }
    .write(dir.path())
    .unwrap();
    let host = EngineHost::start(dir.path()).unwrap();
    assert!(host.is_host());
    assert!(host.handle().is_some());
    let file = concordia_api::transport::connection::ConnectionInfo::read(dir.path())
        .unwrap()
        .unwrap();
    assert_eq!(
        file.host,
        concordia_api::transport::connection::HostKind::Editor
    );
}
