//! Studio checkpoint gate (ADR-0049): `concordia new` and `concordia studio sync` produce the
//! Claude Code files from `studio/`, stay fresh under `--check`, re-sync idempotently, and remove
//! only owned files of a disabled role. Snapshots cover the structure (settings hooks, `.mcp.json`,
//! the `CLAUDE.md` block, which agents and skills exist and their tools); the crate snapshots
//! cover the prompt text.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::SystemTime;

use concordia_tests::bins::concordia_bin;
use concordia_tests::studio_gate::new_project;

fn studio(root: &Path, args: &[&str]) -> Output {
    Command::new(concordia_bin())
        .arg("studio")
        .args(args)
        .current_dir(root)
        .output()
        .expect("run concordia studio")
}

fn read(root: &Path, relative: &str) -> String {
    std::fs::read_to_string(root.join(relative)).unwrap_or_else(|e| panic!("{relative}: {e}"))
}

/// Files directly in `dir` (agents) or one level down (skills), project-relative and sorted.
fn generated(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root.join(".claude/agents")).unwrap() {
        files.push(entry.unwrap().path());
    }
    for entry in std::fs::read_dir(root.join(".claude/skills")).unwrap() {
        files.push(entry.unwrap().path().join("SKILL.md"));
    }
    let mut files: Vec<PathBuf> = files
        .into_iter()
        .map(|p| p.strip_prefix(root).unwrap().to_owned())
        .collect();
    files.sort();
    files
}

/// Each generated agent and skill with its `name:` and `tools:` lines.
fn outline(root: &Path) -> String {
    generated(root)
        .iter()
        .map(|file| {
            let text = read(root, file.to_str().unwrap());
            let keep: Vec<&str> = text
                .lines()
                .filter(|l| l.starts_with("name: ") || l.starts_with("tools: "))
                .collect();
            format!("{}\n  {}", file.display(), keep.join("\n  "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The managed block of `CLAUDE.md`.
fn managed_block(root: &Path) -> String {
    let text = read(root, "CLAUDE.md");
    let start = text
        .find("<!-- concordia-studio:begin -->")
        .expect("begin marker");
    let end = text
        .find("<!-- concordia-studio:end -->")
        .expect("end marker");
    text[start..end].to_owned()
}

fn mtimes(root: &Path) -> Vec<(PathBuf, SystemTime)> {
    let mut files = generated(root);
    files.extend(
        [".claude/settings.json", ".mcp.json", "CLAUDE.md"]
            .iter()
            .map(PathBuf::from),
    );
    files
        .into_iter()
        .map(|f| {
            let time = std::fs::metadata(root.join(&f))
                .unwrap()
                .modified()
                .unwrap();
            (f, time)
        })
        .collect()
}

#[test]
fn m3s_new_project_has_the_claude_code_files_from_studio() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path());
    insta::assert_snapshot!("m3s_agents_and_skills", outline(&root));
    insta::assert_snapshot!("m3s_settings_json", read(&root, ".claude/settings.json"));
    insta::assert_snapshot!("m3s_mcp_json", read(&root, ".mcp.json"));
    insta::assert_snapshot!("m3s_claude_md_block", managed_block(&root));
}

#[test]
fn m3s_sync_is_fresh_and_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path());
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
}

#[test]
fn m3s_disabling_a_role_goes_stale_then_removes_only_its_owned_agent() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path());
    let user_agent = root.join(".claude/agents/my-helper.md");
    std::fs::write(
        &user_agent,
        "---\nname: my-helper\ndescription: mine\n---\nHi.\n",
    )
    .unwrap();
    let team = read(&root, "studio/team.toml");
    std::fs::write(
        root.join("studio/team.toml"),
        format!("{team}\n[roles.writer]\nenabled = false\nreason = \"not needed in this test\"\n"),
    )
    .unwrap();

    let stale = studio(&root, &["sync", "--check"]);
    assert_eq!(stale.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&stale.stdout).contains(".claude/agents/writer.md"),
        "{}",
        String::from_utf8_lossy(&stale.stdout)
    );
    assert!(
        root.join(".claude/agents/writer.md").exists(),
        "--check wrote nothing"
    );

    let synced = studio(&root, &["sync"]);
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    assert!(!root.join(".claude/agents/writer.md").exists());
    assert!(
        user_agent.exists(),
        "a file without the ownership marker is never deleted"
    );
    let block = managed_block(&root);
    assert!(!block.contains("- `writer` ("), "{block}");
    assert!(
        block.contains("`writer` (not needed in this test)"),
        "{block}"
    );
    assert!(studio(&root, &["sync", "--check"]).status.success());
}
