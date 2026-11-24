//! M3 gate: `concordia studio sync` on the fixture game generates the Claude Code files from its
//! `studio/` (snapshots of the structure; the crate snapshots cover the prompt text), stays fresh
//! under `--check`, and re-syncs without touching anything.

use concordia_tests::m3_fixture;
use concordia_tests::studio_gate::{managed_block, mtimes, outline, read, studio};

/// A synced copy of the fixture.
fn synced(dir: &std::path::Path) -> std::path::PathBuf {
    let root = m3_fixture::copy(dir);
    let out = studio(&root, &["sync"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    root
}

#[test]
fn default_studio_sync_snapshots_match() {
    let dir = tempfile::tempdir().unwrap();
    let root = synced(dir.path());
    insta::assert_snapshot!("m3_fixture_agents_and_skills", outline(&root));
    insta::assert_snapshot!(
        "m3_fixture_settings_json",
        read(&root, ".claude/settings.json")
    );
    insta::assert_snapshot!("m3_fixture_mcp_json", read(&root, ".mcp.json"));
    insta::assert_snapshot!("m3_fixture_claude_md_block", managed_block(&root));
}

#[test]
fn fixture_sync_is_idempotent_and_check_clean() {
    let dir = tempfile::tempdir().unwrap();
    let root = synced(dir.path());
    let check = studio(&root, &["sync", "--check"]);
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stdout)
    );
    let before = mtimes(&root);
    std::thread::sleep(std::time::Duration::from_millis(20));
    let again = studio(&root, &["sync"]);
    assert!(
        again.status.success(),
        "{}",
        String::from_utf8_lossy(&again.stderr)
    );
    assert_eq!(mtimes(&root), before, "a second sync rewrote files");
    let roster = studio(&root, &["roster"]);
    assert!(
        roster.status.success(),
        "{}",
        String::from_utf8_lossy(&roster.stderr)
    );
}

/// The fixture's `studio/` is the default roster (one of each role, plus T-0001): every agent
/// and skill file is the same as `concordia studio init` writes today, so the gate tests the
/// shipped prompts, not a stale copy.
#[test]
fn fixture_studio_is_the_default_roster() {
    let fresh = tempfile::tempdir().unwrap();
    concordia_studio::init(fresh.path(), std::path::Path::new("studio")).unwrap();
    let fixture = m3_fixture::fixture().join("studio");
    let roster = concordia_studio::default_roster();
    for (id, text) in roster.roles.iter() {
        let ours = std::fs::read_to_string(fixture.join(format!("agents/{id}.md"))).unwrap();
        assert_eq!(
            &ours, text,
            "studio/agents/{id}.md differs from the default"
        );
    }
    for (name, text) in roster.skills.iter() {
        let ours = std::fs::read_to_string(fixture.join(format!("skills/{name}/SKILL.md")))
            .unwrap_or_else(|_| panic!("studio/skills/{name} is missing"));
        assert_eq!(
            &ours, text,
            "studio/skills/{name}/SKILL.md differs from the default"
        );
    }
    let skills = std::fs::read_dir(fixture.join("skills")).unwrap().count();
    assert_eq!(skills, roster.skills.len(), "no extra skills");
    let team = std::fs::read_to_string(fixture.join("team.toml")).unwrap();
    let default = std::fs::read_to_string(fresh.path().join("studio/team.toml")).unwrap();
    assert_eq!(
        team,
        default.replace("count = 2", "count = 1"),
        "team.toml is the default, one of each"
    );
}
