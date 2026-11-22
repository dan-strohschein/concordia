//! `concordia studio hook pre-tool-use`: stamping and scope checks (M3 phase 3.5, ADR-0080).

mod common;

use common::hook::{decision, pre, project, resync, run};
use concordia_studio::HookEvent::PreToolUse;
use serde_json::json;

#[test]
fn mcp_call_from_a_role_subagent_is_stamped_with_its_identity() {
    let dir = project();
    let input = pre(
        Some("level-designer"),
        "mcp__concordia__entity_spawn",
        json!({"components": {"Name": "Wall"}, "actor": "agent:director"}),
    );
    let (verdict, _, updated) = decision(&run(dir.path(), PreToolUse, &input));
    assert_eq!(verdict, "allow");
    assert_eq!(
        updated["components"],
        json!({"Name": "Wall"}),
        "the rest of the input is kept"
    );
    assert_eq!(updated["_studio"]["role"], "level-designer");
    assert_eq!(updated["_studio"]["agent"], "s-1/a-level-designer");
    assert_eq!(updated["_studio"]["tool_use_id"], "toolu_1");
}

#[test]
fn mcp_call_outside_the_role_scope_is_denied_with_the_allowed_tools() {
    let dir = project();
    let input = pre(
        Some("qa-playtester"),
        "mcp__concordia__component_set",
        json!({}),
    );
    let (verdict, reason, _) = decision(&run(dir.path(), PreToolUse, &input));
    assert_eq!(verdict, "deny");
    assert!(
        reason.contains("qa-playtester") && reason.contains("query_run"),
        "{reason}"
    );
}

#[test]
fn main_session_without_agent_type_acts_as_the_director() {
    let dir = project();
    let input = pre(
        None,
        "mcp__concordia__task_create",
        json!({"title": "x", "role": "writer"}),
    );
    let (verdict, _, updated) = decision(&run(dir.path(), PreToolUse, &input));
    assert_eq!(verdict, "allow");
    assert_eq!(updated["_studio"]["role"], "director");
    assert_eq!(updated["_studio"]["agent"], "s-1/main");
}

#[test]
fn unknown_agent_types_get_read_only_engine_tools() {
    let dir = project();
    let read = pre(
        Some("Explore"),
        "mcp__concordia__query_run",
        json!({"with": ["Name"]}),
    );
    let (verdict, _, updated) = decision(&run(dir.path(), PreToolUse, &read));
    assert_eq!(verdict, "allow");
    assert_eq!(
        updated["_studio"]["role"],
        json!(null),
        "no role: read-only on the server too"
    );
    let write = pre(Some("Explore"), "mcp__concordia__entity_spawn", json!({}));
    assert_eq!(decision(&run(dir.path(), PreToolUse, &write)).0, "deny");
}

#[test]
fn edit_inside_role_write_paths_is_allowed() {
    let dir = project();
    let path = dir.path().join("content/intro.txt");
    let input = pre(
        Some("writer"),
        "Write",
        json!({"file_path": path, "content": "Hi"}),
    );
    assert_eq!(decision(&run(dir.path(), PreToolUse, &input)).0, "allow");
}

#[test]
fn edit_outside_role_write_paths_is_denied() {
    let dir = project();
    let path = dir.path().join("scenes/main.scene.ron");
    let input = pre(
        Some("writer"),
        "Edit",
        json!({"file_path": path, "old_string": "a", "new_string": "b"}),
    );
    let (verdict, reason, _) = decision(&run(dir.path(), PreToolUse, &input));
    assert_eq!(verdict, "deny");
    assert!(
        reason.contains("content/**"),
        "the reason names the role's paths: {reason}"
    );
}

#[test]
fn edit_via_dotdot_or_symlink_escape_is_denied() {
    let dir = project();
    let outside = tempfile::tempdir().unwrap();
    let dotdot = dir.path().join("content/../../escape.txt");
    let input = pre(
        Some("writer"),
        "Write",
        json!({"file_path": dotdot, "content": "x"}),
    );
    assert_eq!(decision(&run(dir.path(), PreToolUse, &input)).0, "deny");

    std::fs::create_dir_all(dir.path().join("content")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), dir.path().join("content/link")).unwrap();
    let linked = dir.path().join("content/link/escape.txt");
    let input = pre(
        Some("writer"),
        "Write",
        json!({"file_path": linked, "content": "x"}),
    );
    assert_eq!(decision(&run(dir.path(), PreToolUse, &input)).0, "deny");
}

#[test]
fn edit_of_a_human_gate_path_is_denied_for_every_agent() {
    let dir = project();
    for (agent, file) in [
        (Some("writer"), "studio/team.toml"),
        (None, ".claude/settings.json"),
        (Some("tech-artist"), "concordia.toml"),
    ] {
        let input = pre(agent, "Edit", json!({"file_path": dir.path().join(file)}));
        let (verdict, reason, _) = decision(&run(dir.path(), PreToolUse, &input));
        assert_eq!(verdict, "deny", "{file}");
        assert!(reason.contains("human"), "{reason}");
    }
}

#[test]
fn edit_of_a_denied_path_is_denied() {
    let dir = project();
    let team = dir.path().join("studio/team.toml");
    let text = std::fs::read_to_string(&team).unwrap().replace(
        "deny = [\".env\", \"**/*.key\"]",
        "deny = [\".env\", \"**/*.key\", \"content/secret/**\"]",
    );
    std::fs::write(&team, text).unwrap();
    resync(dir.path());
    let input = pre(
        Some("writer"),
        "Write",
        json!({"file_path": dir.path().join("content/secret/a.txt")}),
    );
    assert_eq!(decision(&run(dir.path(), PreToolUse, &input)).0, "deny");
}

#[test]
fn bash_command_not_in_the_allowlist_is_denied_for_a_scoped_role() {
    let dir = project();
    let allowed = pre(
        Some("gameplay-programmer"),
        "Bash",
        json!({"command": "cargo test -p game"}),
    );
    assert_eq!(decision(&run(dir.path(), PreToolUse, &allowed)).0, "allow");
    let denied = pre(
        Some("gameplay-programmer"),
        "Bash",
        json!({"command": "rm -rf studio"}),
    );
    let (verdict, reason, _) = decision(&run(dir.path(), PreToolUse, &denied));
    assert_eq!(verdict, "deny");
    assert!(reason.contains("cargo test *"), "{reason}");
    let no_bash = pre(Some("writer"), "Bash", json!({"command": "ls"}));
    assert_eq!(
        decision(&run(dir.path(), PreToolUse, &no_bash)).0,
        "deny",
        "writer has no Bash"
    );
}

#[test]
fn a_stale_policy_denies_with_a_sync_hint() {
    let dir = project();
    let role = dir.path().join("studio/agents/writer.md");
    std::fs::write(&role, std::fs::read_to_string(&role).unwrap() + "\nMore.\n").unwrap();
    let input = pre(Some("writer"), "mcp__concordia__task_list", json!({}));
    let (verdict, reason, _) = decision(&run(dir.path(), PreToolUse, &input));
    assert_eq!(verdict, "deny");
    assert!(reason.contains("concordia studio sync"), "{reason}");
}

#[test]
fn tools_outside_studio_are_left_to_claude_code() {
    let dir = project();
    let input = pre(Some("writer"), "WebSearch", json!({"query": "x"}));
    assert_eq!(decision(&run(dir.path(), PreToolUse, &input)).0, "");
}

#[test]
fn hook_fails_closed_on_malformed_input() {
    let dir = project();
    let outcome = concordia_studio::hook::run(PreToolUse, "{not json", dir.path());
    assert_eq!(outcome.exit, 2);
    assert!(outcome.stderr.contains("hook input"), "{}", outcome.stderr);
}

#[test]
fn animator_writes_only_anim_files_and_model_sidecars() {
    let dir = project();
    let verdict = |path: &str| {
        let input = pre(
            Some("animator"),
            "Write",
            json!({"file_path": dir.path().join(path), "content": "x"}),
        );
        decision(&run(dir.path(), PreToolUse, &input)).0
    };
    for allowed in [
        "assets/anim/hero.animgraph.ron",
        "assets/anim/previews/hero.scene.ron",
        "assets/models/hero.fbx.import.ron",
        "assets/models/people/guard.glb.import.ron",
    ] {
        assert_eq!(verdict(allowed), "allow", "{allowed}");
    }
    for denied in [
        "assets/models/hero.fbx",
        "assets/models/hero.fbx.import.ron.bak",
        "assets/blender/hero.py",
        "scenes/main.scene.ron",
        "scripts/hero/src/lib.rs",
        "assets/materials/skin.material.ron",
    ] {
        assert_eq!(verdict(denied), "deny", "{denied}");
    }
}
