//! `concordia mcp` with Agent Studio: per-agent sessions, server-side scopes, budgets (M3 phase 3.5).

mod common;

use common::{Client, call, connect_with, options, text};
use rmcp::model::ClientConfig;
use serde_json::{Value, json};
use std::path::Path;

/// A Studio project (default team, level-designer ×2) with an empty scene open.
async fn studio(dir: &tempfile::TempDir, team_edit: impl Fn(String) -> String) -> Client {
    concordia_studio::init(dir.path(), Path::new("studio")).unwrap();
    let team = dir.path().join("studio/team.toml");
    std::fs::write(&team, team_edit(std::fs::read_to_string(&team).unwrap())).unwrap();
    std::fs::write(
        dir.path().join("concordia.toml"),
        "[physics]\n\n[assets]\n\n[anim]\n\n[render]\n\n[scripts]\n\n[replay]\n\n[studio]\n",
    )
    .unwrap();
    let mut opts = options(dir);
    opts.modules = concordia_app::Project::load(dir.path())
        .unwrap()
        .modules()
        .unwrap();
    opts.studio = Some("studio".into());
    let client = connect_with(opts, ClientConfig::default()).await;
    let scene = call(&client, "scene_new", json!({"path": "scenes/a.scene.ron", "name": "A", "_studio": envelope("level-designer", "s/1")})).await;
    assert_ne!(scene.is_error, Some(true), "{}", text(&scene));
    client
}

fn envelope(role: &str, agent: &str) -> Value {
    json!({"role": role, "agent": agent, "tool_use_id": "t"})
}

fn with(args: Value, role: &str, agent: &str) -> Value {
    let mut args = args;
    args["_studio"] = envelope(role, agent);
    args
}

async fn history(client: &Client) -> Vec<Value> {
    let result = call(client, "txn_history", json!({})).await;
    result.structured_content.unwrap()["entries"]
        .as_array()
        .unwrap()
        .clone()
}

#[tokio::test]
async fn edits_are_attributed_to_the_role_instance_not_the_model_supplied_actor() {
    let dir = tempfile::tempdir().unwrap();
    let client = studio(&dir, |t| t).await;
    let spawn = with(
        json!({"components": {"Name": "Wall"}, "actor": "agent:director"}),
        "level-designer",
        "s/1",
    );
    let result = call(&client, "entity_spawn", spawn).await;
    assert_ne!(result.is_error, Some(true), "{}", text(&result));
    let last = history(&client).await.last().unwrap().clone();
    assert_eq!(last["actor"], "agent:level-designer#1");
}

#[tokio::test]
async fn each_agent_gets_its_own_session_and_instance_number() {
    let dir = tempfile::tempdir().unwrap();
    let client = studio(&dir, |t| t).await;
    let begin = call(
        &client,
        "txn_begin",
        with(json!({"label": "walls"}), "level-designer", "s/1"),
    )
    .await;
    assert_ne!(begin.is_error, Some(true), "{}", text(&begin));
    // A second level designer is #2, in its own session: it can't join #1's transaction.
    let other = call(
        &client,
        "entity_spawn",
        with(
            json!({"components": {"Name": "X"}}),
            "level-designer",
            "s/2",
        ),
    )
    .await;
    assert_eq!(other.is_error, Some(true));
    assert!(text(&other).contains("TxnConflict"), "{}", text(&other));
    assert!(
        text(&other).contains("agent:level-designer#1"),
        "names the holder: {}",
        text(&other)
    );
}

#[tokio::test]
async fn instance_slots_are_capped_at_the_role_count() {
    let dir = tempfile::tempdir().unwrap();
    let client = studio(&dir, |t| t).await;
    for agent in ["s/1", "s/2"] {
        let ok = call(
            &client,
            "scene_info",
            with(json!({}), "level-designer", agent),
        )
        .await;
        assert_ne!(ok.is_error, Some(true), "{}", text(&ok));
    }
    let third = call(
        &client,
        "scene_info",
        with(json!({}), "level-designer", "s/3"),
    )
    .await;
    assert_eq!(third.is_error, Some(true));
    assert!(text(&third).contains("capacity"), "{}", text(&third));
}

#[tokio::test]
async fn server_rejects_a_tool_outside_the_role_scope_even_without_the_hook() {
    let dir = tempfile::tempdir().unwrap();
    let client = studio(&dir, |t| t).await;
    let forged = call(
        &client,
        "entity_spawn",
        with(json!({"components": {}}), "qa-playtester", "s/9"),
    )
    .await;
    assert_eq!(forged.is_error, Some(true));
    assert!(text(&forged).contains("NotPermitted"), "{}", text(&forged));
}

#[tokio::test]
async fn calls_without_the_studio_envelope_are_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let client = studio(&dir, |t| t).await;
    let read = call(&client, "scene_info", json!({})).await;
    assert_ne!(read.is_error, Some(true), "{}", text(&read));
    let write = call(&client, "entity_spawn", json!({"components": {}})).await;
    assert_eq!(write.is_error, Some(true));
    assert!(
        text(&write).contains("hook"),
        "explains the missing hook: {}",
        text(&write)
    );
}

#[tokio::test]
async fn server_rejects_scene_writes_outside_the_role_paths() {
    let dir = tempfile::tempdir().unwrap();
    let client = studio(&dir, |t| t).await;
    let outside = call(
        &client,
        "scene_save",
        with(
            json!({"path": "content/a.scene.ron"}),
            "level-designer",
            "s/1",
        ),
    )
    .await;
    assert_eq!(outside.is_error, Some(true));
    assert!(text(&outside).contains("scenes/**"), "{}", text(&outside));
    let inside = call(
        &client,
        "scene_save",
        with(
            json!({"path": "scenes/b.scene.ron"}),
            "level-designer",
            "s/1",
        ),
    )
    .await;
    assert_ne!(inside.is_error, Some(true), "{}", text(&inside));
}

#[tokio::test]
async fn server_rejects_material_writes_outside_the_role_paths() {
    let dir = tempfile::tempdir().unwrap();
    let client = studio(&dir, |t| t).await;
    let material = json!({"nodes": {"surface": {"Slab": {"roughness": 0.4}}}, "root": "surface"});
    for path in [
        "scenes/x.material.ron",
        "studio/agents/x.material.ron",
        "assets/x.material.ron",
    ] {
        let outside = call(
            &client,
            "material_set",
            with(
                json!({"path": path, "material": material}),
                "tech-artist",
                "s/2",
            ),
        )
        .await;
        assert_eq!(outside.is_error, Some(true), "{path}: {}", text(&outside));
        assert!(!dir.path().join(path).exists(), "{path} was not written");
    }
    let inside = call(
        &client,
        "material_set",
        with(
            json!({"path": "assets/materials/x.material.ron", "material": material}),
            "tech-artist",
            "s/2",
        ),
    )
    .await;
    assert_ne!(inside.is_error, Some(true), "{}", text(&inside));
}

#[tokio::test]
async fn human_actors_are_still_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let client = studio(&dir, |t| t).await;
    let result = call(
        &client,
        "scene_info",
        with(json!({"actor": "human:me"}), "level-designer", "s/1"),
    )
    .await;
    assert_eq!(result.is_error, Some(true));
}

#[tokio::test]
async fn per_task_mutation_budget_exhaustion_blocks_the_task() {
    let dir = tempfile::tempdir().unwrap();
    let client = studio(&dir, |t| {
        t.replace("max_mutations_per_task = 200", "max_mutations_per_task = 2")
    })
    .await;
    let created = call(
        &client,
        "task_create",
        with(
            json!({"title": "Walls", "role": "level-designer"}),
            "director",
            "s/main",
        ),
    )
    .await;
    assert_ne!(created.is_error, Some(true), "{}", text(&created));
    call(
        &client,
        "task_claim",
        with(json!({"id": "T-0001"}), "level-designer", "s/1"),
    )
    .await;
    for i in 0..2 {
        let ok = call(
            &client,
            "entity_spawn",
            with(
                json!({"components": {"Name": format!("W{i}")}}),
                "level-designer",
                "s/1",
            ),
        )
        .await;
        assert_ne!(ok.is_error, Some(true), "{}", text(&ok));
    }
    let over = call(
        &client,
        "entity_spawn",
        with(json!({"components": {}}), "level-designer", "s/1"),
    )
    .await;
    assert_eq!(over.is_error, Some(true));
    assert!(text(&over).contains("budget"), "{}", text(&over));
    let task = call(
        &client,
        "task_get",
        with(json!({"id": "T-0001"}), "director", "s/main"),
    )
    .await;
    let task = task.structured_content.unwrap();
    assert_eq!(task["task"]["status"], "blocked");
    assert_eq!(task["task"]["assignee"], "human:local");
}

#[tokio::test]
async fn server_holds_the_animator_to_its_paths() {
    let dir = tempfile::tempdir().unwrap();
    let client = studio(&dir, |t| t).await;
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fbx/skinned.fbx");
    for at in ["assets/models/hero.fbx", "content/hero.fbx"] {
        std::fs::create_dir_all(dir.path().join(at).parent().unwrap()).unwrap();
        std::fs::copy(&fixture, dir.path().join(at)).unwrap();
    }
    let animator = |args: Value| with(args, "animator", "s/7");
    // Importing writes only the model's sidecar, which the animator may write (clip events).
    let model = call(
        &client,
        "asset_import",
        animator(json!({"path": "assets/models/hero.fbx"})),
    )
    .await;
    assert_ne!(model.is_error, Some(true), "{}", text(&model));
    assert!(
        dir.path()
            .join("assets/models/hero.fbx.import.ron")
            .is_file()
    );
    let outside = call(
        &client,
        "asset_import",
        animator(json!({"path": "content/hero.fbx"})),
    )
    .await;
    assert_eq!(outside.is_error, Some(true), "{}", text(&outside));
    assert!(
        text(&outside).contains("assets/anim/**"),
        "{}",
        text(&outside)
    );
    assert!(!dir.path().join("content/hero.fbx.import.ron").exists());
    // Another role still needs the model's folder: the tech artist may not import it.
    let artist = call(
        &client,
        "asset_import",
        with(
            json!({"path": "assets/models/hero.fbx"}),
            "tech-artist",
            "s/2",
        ),
    )
    .await;
    assert_eq!(artist.is_error, Some(true), "{}", text(&artist));

    // Previews live under assets/anim/; game scenes are the level designer's.
    let preview = call(
        &client,
        "scene_new",
        animator(json!({"path": "assets/anim/previews/hero.scene.ron", "name": "Hero preview"})),
    )
    .await;
    assert_ne!(preview.is_error, Some(true), "{}", text(&preview));
    let saved = call(&client, "scene_save", animator(json!({}))).await;
    assert_ne!(saved.is_error, Some(true), "{}", text(&saved));
    assert!(
        dir.path()
            .join("assets/anim/previews/hero.scene.ron")
            .is_file()
    );
    for path in ["scenes/hero.scene.ron", "assets/models/hero.scene.ron"] {
        let game = call(&client, "scene_save", animator(json!({"path": path}))).await;
        assert_eq!(game.is_error, Some(true), "{path}: {}", text(&game));
        assert!(!dir.path().join(path).exists(), "{path}");
    }
    let material = call(
        &client,
        "material_set",
        animator(json!({"path": "assets/anim/x.material.ron", "material": {"nodes": {"s": {"Slab": {}}}, "root": "s"}})),
    )
    .await;
    assert_eq!(material.is_error, Some(true));
    assert!(
        text(&material).contains("NotPermitted"),
        "{}",
        text(&material)
    );
}

#[tokio::test]
async fn tech_artist_saves_the_scene_it_assigned_materials_in() {
    let dir = tempfile::tempdir().unwrap();
    let client = studio(&dir, |t| t).await;
    // Scenes are shared (studio/team.toml): the level designer edits them but doesn't own them alone.
    let saved = call(&client, "scene_save", with(json!({}), "tech-artist", "s/2")).await;
    assert_ne!(saved.is_error, Some(true), "{}", text(&saved));
    assert!(dir.path().join("scenes/a.scene.ron").is_file());
    let elsewhere = call(
        &client,
        "scene_save",
        with(json!({"path": "content/a.scene.ron"}), "tech-artist", "s/2"),
    )
    .await;
    assert_eq!(elsewhere.is_error, Some(true), "{}", text(&elsewhere));
}
