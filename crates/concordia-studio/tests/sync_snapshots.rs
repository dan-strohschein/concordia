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
