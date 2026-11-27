//! Animation tools over MCP (M2 phase 2.4.4): `anim_inspect` is served with generated schemas when
//! `[anim]` is on, and an agent inspects an imported character's pose in a running game.

use crate::common;

use std::path::Path;

use common::{call, connect_with, options, text};
use concordia_api::{MethodInfo, params_schema, result_schema};
use rmcp::model::CallToolResult;
use serde_json::{Value, json};

fn json_of(result: &CallToolResult) -> Value {
    assert_ne!(result.is_error, Some(true), "{}", text(result));
    serde_json::from_str(&text(result)).unwrap()
}

/// A project with `[assets]` and `[anim]`, the FBX character as `hero.fbx` and its actions as
/// `moves.fbx`.
async fn project() -> (tempfile::TempDir, common::Client) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("concordia.toml"), "[assets]\n\n[anim]\n").unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fbx");
    for (from, to) in [
        ("skinned.fbx", "hero.fbx"),
        ("anim_stacks.fbx", "moves.fbx"),
    ] {
        std::fs::copy(fixtures.join(from), dir.path().join(to)).unwrap();
    }
    let mut options = options(&dir);
    options.modules = concordia_app::Project::load(dir.path())
        .and_then(|p| p.modules())
        .unwrap();
    let client = connect_with(options, Default::default()).await;
    (dir, client)
}

#[tokio::test]
async fn anim_methods_have_generated_schemas_and_mcp_tools() {
    let (_dir, client) = project().await;
    let tools = client.list_all_tools().await.unwrap();
    let inspect = tools
        .iter()
        .find(|t| t.name == "anim_inspect")
        .expect("anim_inspect is offered with [anim]");
    let method = MethodInfo::of::<concordia_anim::AnimInspect>();
    assert_eq!(
        serde_json::to_value(inspect.input_schema.as_ref()).unwrap(),
        params_schema(&method)
    );
    assert_eq!(
        serde_json::to_value(inspect.output_schema.as_ref().expect("an output schema")).unwrap(),
        result_schema(&method)
    );
    assert_eq!(
        inspect.annotations.as_ref().and_then(|a| a.read_only_hint),
        Some(true)
    );
    for dropped in ["anim_set_param", "anim_travel"] {
        assert!(
            tools.iter().all(|t| t.name != dropped),
            "{dropped}: component_patch with world play does it (2.4.4)"
        );
    }
}

#[tokio::test]
async fn agent_inspects_a_pose_over_mcp() {
    let (_dir, client) = project().await;
    let ok = |result: CallToolResult| json_of(&result);
    ok(call(
        &client,
        "scene_new",
        json!({"path": "scenes/a.scene.ron", "name": "A"}),
    )
    .await);
    let hero = ok(call(&client, "asset_import", json!({"path": "hero.fbx"})).await)["id"].clone();
    let moves = ok(call(&client, "asset_import", json!({"path": "moves.fbx"})).await)["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let made = ok(call(
        &client,
        "prefab_instantiate",
        json!({"asset": format!("asset:{}", hero.as_str().unwrap())}),
    )
    .await);
    let mut body = Value::Null;
    for e in made["entities"].as_array().unwrap() {
        let got = ok(call(&client, "entity_get", json!({"id": e})).await);
        if got["components"].get("Skinned").is_some() {
            body = e.clone();
        }
    }
    assert!(body.is_string(), "the instance has a skinned body");
    let clip = format!("asset:{moves}#clip/Armature|Walk");
    ok(call(
        &client,
        "component_set",
        json!({"entity": body, "component": "AnimPlayer", "value": {"clip": clip}}),
    )
    .await);

    let early = call(&client, "anim_inspect", json!({"entity": body})).await;
    assert_eq!(early.is_error, Some(true), "nothing plays before sim_step");
    assert!(text(&early).contains("NotPlaying"), "{}", text(&early));

    ok(call(&client, "sim_step", json!({"ticks": 15})).await);
    let seen = ok(call(&client, "anim_inspect", json!({"entity": body})).await);
    let bones = seen["bones"].as_array().unwrap();
    assert_eq!(bones.len(), 7, "{seen}");
    assert!(
        bones
            .iter()
            .all(|b| b["name"].is_string() && b["world"]["translation"].is_array())
    );
    assert!(
        bones.iter().filter(|b| b["parent"].is_null()).count() >= 1,
        "a root bone"
    );
    let player = &seen["clip"];
    assert_eq!(player["clip"], json!(clip));
    let time = player["time"].as_f64().unwrap();
    assert!((time - 0.25).abs() < 1e-4, "15 ticks at 60 Hz: {player}");
    let frame = player["frame"].as_f64().unwrap();
    assert!(
        (frame - 6.0).abs() < 1e-3,
        "0.25 s at the file's 24 fps: {player}"
    );
    assert!(seen["graph"].is_null());

    // Steering the same player with component_patch (world play) shows up on the next inspect.
    ok(call(
        &client,
        "component_patch",
        json!({"entity": body, "component": "AnimPlayer", "patch": {"speed": 0.0}, "world": "play"}),
    )
    .await);
    ok(call(&client, "sim_step", json!({"ticks": 5})).await);
    let held = ok(call(&client, "anim_inspect", json!({"entity": body})).await);
    assert_eq!(held["clip"]["speed"], json!(0.0));
    assert_eq!(held["clip"]["time"], player["time"], "speed 0 holds");

    let first = made["entities"][0].clone();
    if first != body {
        let refused = call(&client, "anim_inspect", json!({"entity": first})).await;
        assert_eq!(refused.is_error, Some(true));
        assert!(text(&refused).contains("Skinned"), "{}", text(&refused));
    }
}
