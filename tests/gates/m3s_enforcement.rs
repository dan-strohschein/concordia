//! Studio checkpoint gate (ADR-0049, ADR-0080): per-role enforcement through the real hook binary
//! and the real `concordia mcp`, with hook inputs shaped like Claude Code's. The hooks block
//! out-of-scope writes, tools and shells; the server enforces the same scopes and stamps each
//! role's actor itself, whatever the model claims. No live agent, no API key.

use std::path::Path;

use concordia_studio::StudioJournal;
use concordia_tests::studio_gate::{
    SESSION, agent_call, agent_try_as, decision, hook, hook_raw, input, mcp, new_project,
    pre_tool_use,
};
use serde_json::{Value, json};

fn edit(root: &Path, role: &str, path: &str) -> String {
    let file = root.join(path).display().to_string();
    let out = pre_tool_use(
        root,
        Some(role),
        &format!("agent-{role}"),
        "Edit",
        json!({"file_path": file}),
    );
    decision(&out).to_owned()
}

#[test]
fn m3s_hooks_block_out_of_scope_writes_tools_and_shells() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path());
    assert_eq!(
        edit(&root, "level-designer", "scenes/arena.scene.ron"),
        "allow"
    );
    assert_eq!(
        edit(&root, "qa-playtester", "scenes/arena.scene.ron"),
        "deny"
    );
    assert_eq!(
        edit(&root, "level-designer", "scenes/../studio/team.toml"),
        "deny"
    );
    for role in ["level-designer", "tech-artist", "writer"] {
        assert_eq!(
            edit(&root, role, "studio/team.toml"),
            "deny",
            "{role}: human-only gate path"
        );
        assert_eq!(
            edit(&root, role, ".claude/settings.json"),
            "deny",
            "{role}: human-only gate path"
        );
    }

    let shell = pre_tool_use(
        &root,
        None,
        "main",
        "Bash",
        json!({"command": "rm -rf scenes"}),
    );
    assert_eq!(
        decision(&shell),
        "deny",
        "the director has no shell: {shell}"
    );

    let tool = pre_tool_use(
        &root,
        Some("qa-playtester"),
        "agent-qa",
        "mcp__concordia__component_set",
        json!({"entity": "x", "component": "Name", "value": "y"}),
    );
    assert_eq!(decision(&tool), "deny", "{tool}");
    let reason = tool["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .unwrap();
    assert!(
        reason.contains("component_get"),
        "the reason lists allowed tools: {reason}"
    );

    let broken = hook_raw(&root, "pre-tool-use", "{not json");
    assert_eq!(broken.code, Some(2), "PreToolUse fails closed");
    assert!(!broken.stderr.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn m3s_server_stamps_actors_and_enforces_scopes_without_the_hook() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path());
    let mcp = mcp(&root).await;
    agent_call(
        &root,
        &mcp,
        Some("level-designer"),
        "scene_open",
        json!({"path": "scenes/main.scene.ron"}),
    )
    .await;

    // A spoofed actor is ignored: the stamp decides who acted.
    let spawned = agent_call(
        &root,
        &mcp,
        Some("level-designer"),
        "entity_spawn",
        json!({"components": {"Name": "Wall"}, "actor": "agent:director"}),
    )
    .await;
    assert!(spawned["id"].is_string(), "{spawned}");
    let history = mcp.call("txn_history", json!({})).await.unwrap();
    let last = history["entries"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["actor"], "agent:level-designer#1", "{last}");

    // Without the hook's stamp, a write is refused; reads still work.
    let unstamped = mcp
        .call("entity_spawn", json!({"components": {"Name": "Sneaky"}}))
        .await
        .unwrap_err();
    assert!(unstamped.text.contains("read-only"), "{}", unstamped.text);
    mcp.call("scene_info", json!({})).await.unwrap();

    // A stamp the hook would never give (QA writing components) is rejected by the server too.
    let id = spawned["id"].clone();
    let forged = json!({
        "entity": id, "component": "Name", "value": "Renamed",
        "_studio": {"role": "qa-playtester", "agent": format!("{SESSION}/agent-qa"), "tool_use_id": "forged"},
    });
    let refused = mcp.call("component_set", forged).await.unwrap_err();
    assert!(refused.text.contains("component_set"), "{}", refused.text);

    // The level designer has two instances: a third concurrent one is at capacity.
    for agent in ["agent-ld-2", "agent-ld-3"] {
        let result = agent_try_as(
            &root,
            &mcp,
            Some("level-designer"),
            agent,
            "entity_spawn",
            json!({"components": {"Name": agent}}),
        )
        .await;
        match agent {
            "agent-ld-2" => assert!(
                result.is_ok(),
                "{agent}: {:?}",
                result.err().map(|e| e.text)
            ),
            _ => {
                let err = result.unwrap_err().text;
                assert!(err.contains("at capacity"), "{agent}: {err}");
            }
        }
    }
    mcp.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn m3s_file_edits_are_journaled_with_the_role_actor() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path());
    let mcp = mcp(&root).await;
    // An engine call first, so the designer holds a slot (#1) like a real session would.
    agent_call(&root, &mcp, Some("level-designer"), "schema_get", json!({})).await;

    let file = root.join("scenes/notes.scene.ron");
    let edit_input = json!({"file_path": file.display().to_string()});
    let pre = pre_tool_use(
        &root,
        Some("level-designer"),
        "agent-level-designer",
        "Write",
        edit_input.clone(),
    );
    assert_eq!(decision(&pre), "allow", "{pre}");
    std::fs::write(&file, "// notes\n").unwrap();
    let mut post: Value = input("PostToolUse", Some("level-designer"));
    post["tool_name"] = json!("Write");
    post["tool_input"] = edit_input;
    post["tool_use_id"] = json!("toolu-Write-agent-level-designer");
    post["tool_response"] = json!({"success": true});
    hook(&root, "post-tool-use", &post);

    let entries = StudioJournal::of_project(&root).read(None).unwrap();
    let edit = entries
        .iter()
        .map(|e| e.to_json())
        .find(|e| e["kind"] == "file_edit")
        .expect("a file_edit entry");
    assert_eq!(edit["path"], "scenes/notes.scene.ron");
    assert_eq!(edit["tool"], "Write");
    assert!(
        edit["actor"]
            .as_str()
            .unwrap()
            .starts_with("agent:level-designer"),
        "{edit}"
    );
    assert!(
        edit["after"].is_string() && edit["before"].is_null(),
        "{edit}"
    );
    mcp.shutdown().await;
}
