//! `studio sync` output for the default roster, one snapshot per generated file (M3 phase 3.4).

mod common;

use common::sync::{agent_files, project, read, write};

#[test]
fn sync_default_roster_generates_expected_agent_files() {
    let dir = project();
    write(dir.path());
    let names: Vec<String> = agent_files(dir.path())
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        [
            "animator.md",
            "composer.md",
            "director.md",
            "gameplay-programmer.md",
            "level-designer.md",
            "modeler.md",
            "qa-playtester.md",
            "sound-designer.md",
            "tech-artist.md",
            "ui-ux.md",
            "writer.md"
        ],
        "one file per enabled role"
    );
    for file in agent_files(dir.path()) {
        let name = file.file_stem().unwrap().to_string_lossy().into_owned();
        insta::assert_snapshot!(
            format!("agent_{name}"),
            std::fs::read_to_string(&file).unwrap()
        );
    }
}

#[test]
fn sync_generates_expected_settings_json_hooks_and_env() {
    let dir = project();
    write(dir.path());
    insta::assert_snapshot!(read(dir.path(), ".claude/settings.json"));
}

#[test]
fn sync_generates_expected_mcp_json() {
    let dir = project();
    write(dir.path());
    insta::assert_snapshot!(read(dir.path(), ".mcp.json"));
}

#[test]
fn sync_generates_expected_claude_md_block() {
    let dir = project();
    write(dir.path());
    insta::assert_snapshot!(read(dir.path(), "CLAUDE.md"));
}

#[test]
fn sync_generates_expected_skill_files() {
    let dir = project();
    write(dir.path());
    insta::assert_snapshot!(read(dir.path(), ".claude/skills/concordia-demo/SKILL.md"));
}

#[test]
fn studio_sync_emits_graph_skills_and_permissions() {
    let dir = project();
    write(dir.path());
    insta::assert_snapshot!(
        "skill_author_graph",
        read(dir.path(), ".claude/skills/concordia-author-graph/SKILL.md")
    );
    insta::assert_snapshot!(
        "skill_author_behavior",
        read(
            dir.path(),
            ".claude/skills/concordia-author-behavior/SKILL.md"
        )
    );
    let policy: serde_json::Value =
        serde_json::from_str(&read(dir.path(), ".claude/concordia/policy.json")).unwrap();
    for role in ["gameplay-programmer", "level-designer"] {
        let r = &policy["roles"][role];
        let writes: Vec<&str> = r["write"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w.as_str().unwrap())
            .collect();
        assert!(
            writes.contains(&"graphs/**"),
            "{role} writes graphs: {writes:?}"
        );
        let tools: Vec<&str> = r["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap())
            .collect();
        for tool in [
            "graph_list_nodes",
            "graph_read",
            "graph_check",
            "graph_compile",
            "graph_apply",
            "graph_trace",
        ] {
            assert!(
                tools.iter().any(|t| t.ends_with(tool)),
                "{role} has {tool}: {tools:?}"
            );
        }
    }
    // Other roles don't edit graphs.
    let writes = policy["roles"]["technical-artist"]["write"].to_string();
    assert!(!writes.contains("graphs"), "{writes}");
}
