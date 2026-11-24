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
