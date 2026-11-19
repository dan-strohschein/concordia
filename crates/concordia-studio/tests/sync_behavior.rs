//! `studio sync` behavior: idempotence, merging, ownership, check mode (M3 phase 3.4).

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::sync::{agent_files, frontmatter_keys, project, read, run, tools, write};
use concordia_studio::{CLAUDE_AGENT_FIELDS, SyncAction, SyncOptions};
use serde_json::Value;

#[test]
fn sync_is_idempotent_second_run_writes_nothing() {
    let dir = project();
    assert!(!write(dir.path()).changes.is_empty());
    let mtimes = |root: &Path| -> Vec<_> {
        agent_files(root)
            .iter()
            .chain(
                [
                    root.join(".claude/settings.json"),
                    root.join(".mcp.json"),
                    root.join("CLAUDE.md"),
                ]
                .iter(),
            )
            .map(|p| std::fs::metadata(p).unwrap().modified().unwrap())
            .collect()
    };
    let before = mtimes(dir.path());
    std::thread::sleep(std::time::Duration::from_millis(20));
    let second = write(dir.path());
    assert!(
        second
            .changes
            .iter()
            .all(|c| c.action == SyncAction::Unchanged),
        "{:?}",
        second.changes
    );
    assert_eq!(mtimes(dir.path()), before);
}

#[test]
fn sync_preserves_unowned_settings_hooks_env_and_mcp_servers() {
    let dir = project();
    let settings = r#"{
  "model": "opus",
  "env": {
    "MY_VAR": "1"
  },
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "my-lint"
          }
        ]
      }
    ]
  },
  "permissions": {
    "deny": [
      "Read(./.env)"
    ]
  }
}
"#;
    std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
    std::fs::write(dir.path().join(".claude/settings.json"), settings).unwrap();
    std::fs::write(
        dir.path().join(".mcp.json"),
        "{\n  \"mcpServers\": {\n    \"other\": {\n      \"command\": \"other-server\"\n    }\n  }\n}\n",
    )
    .unwrap();
    write(dir.path());
    write(dir.path());

    let merged: Value = serde_json::from_str(&read(dir.path(), ".claude/settings.json")).unwrap();
    assert_eq!(merged["model"], "opus");
    assert_eq!(merged["env"]["MY_VAR"], "1");
    assert_eq!(merged["permissions"]["deny"][0], "Read(./.env)");
    let pre = merged["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(
        pre[0]["hooks"][0]["command"], "my-lint",
        "the user's hook stays first"
    );
    let ours = pre
        .iter()
        .filter(|g| g["hooks"][0]["command"] == "concordia")
        .count();
    assert_eq!(ours, 3, "no duplicates after two syncs");
    let text = read(dir.path(), ".claude/settings.json");
    let order: Vec<usize> = ["\"model\"", "\"env\"", "\"hooks\"", "\"permissions\""]
        .iter()
        .map(|k| text.find(k).unwrap())
        .collect();
    assert!(
        order.windows(2).all(|w| w[0] < w[1]),
        "key order kept:\n{text}"
    );

    let mcp: Value = serde_json::from_str(&read(dir.path(), ".mcp.json")).unwrap();
    assert_eq!(mcp["mcpServers"]["other"]["command"], "other-server");
    assert_eq!(mcp["mcpServers"]["concordia"]["command"], "concordia");
}

#[test]
fn sync_preserves_claude_md_outside_markers() {
    let dir = project();
    std::fs::write(dir.path().join("CLAUDE.md"), "# My game\n\nHouse rules.\n").unwrap();
    write(dir.path());
    let first = read(dir.path(), "CLAUDE.md");
    assert!(first.starts_with("# My game\n\nHouse rules.\n"), "{first}");
    let edited = first.replace("House rules.", "House rules, edited.") + "\nTrailing notes.\n";
    std::fs::write(dir.path().join("CLAUDE.md"), &edited).unwrap();
    write(dir.path());
    assert_eq!(read(dir.path(), "CLAUDE.md"), edited);
    assert_eq!(edited.matches("<!-- concordia-studio:begin -->").count(), 1);
}

#[test]
fn sync_refuses_to_overwrite_hand_edited_agent_file_without_force() {
    let dir = project();
    write(dir.path());
    let path = dir.path().join(".claude/agents/writer.md");
    let edited = std::fs::read_to_string(&path).unwrap() + "\nMy tweak.\n";
    std::fs::write(&path, &edited).unwrap();
    // Change the source so the file is stale.
    let role = dir.path().join("studio/agents/writer.md");
    std::fs::write(&role, std::fs::read_to_string(&role).unwrap() + "\nMore.\n").unwrap();

    let refused = write(dir.path());
    assert_eq!(refused.refused, [".claude/agents/writer.md"]);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), edited);

    let forced = run(
        dir.path(),
        SyncOptions {
            force: true,
            ..SyncOptions::default()
        },
    );
    assert!(forced.refused.is_empty());
    assert!(std::fs::read_to_string(&path).unwrap().contains("More."));
}

#[test]
fn sync_removes_agent_file_for_disabled_role_only_if_owned() {
    let dir = project();
    write(dir.path());
    let mine = dir.path().join(".claude/agents/my-helper.md");
    std::fs::write(&mine, "---\nname: my-helper\ndescription: mine\n---\nHi\n").unwrap();
    let team = dir.path().join("studio/team.toml");
    let text = std::fs::read_to_string(&team).unwrap();
    std::fs::write(
        &team,
        format!("{text}\n[roles.writer]\nenabled = false\nreason = \"no text yet\"\n"),
    )
    .unwrap();

    let report = write(dir.path());
    assert!(
        report
            .changes
            .iter()
            .any(|c| c.path == ".claude/agents/writer.md" && c.action == SyncAction::Delete),
        "{:?}",
        report.changes
    );
    assert!(!dir.path().join(".claude/agents/writer.md").exists());
    assert!(
        mine.exists(),
        "a file without the ownership marker is never deleted"
    );
}

#[test]
fn sync_check_reports_stale_files_with_a_diff_and_writes_nothing() {
    let dir = project();
    let report = run(
        dir.path(),
        SyncOptions {
            check: true,
            ..SyncOptions::default()
        },
    );
    assert!(report.is_stale());
    assert!(
        !dir.path().join(".claude").exists(),
        "check mode writes nothing"
    );
    write(dir.path());
    let role = dir.path().join("studio/agents/writer.md");
    std::fs::write(
        &role,
        std::fs::read_to_string(&role)
            .unwrap()
            .replace("Give the game its words", "Write all text"),
    )
    .unwrap();
    let stale = run(
        dir.path(),
        SyncOptions {
            check: true,
            ..SyncOptions::default()
        },
    );
    let change = stale
        .changes
        .iter()
        .find(|c| c.action == SyncAction::Update)
        .unwrap();
    assert_eq!(change.path, ".claude/agents/writer.md");
    let diff = change.diff.as_deref().unwrap();
    assert!(
        diff.contains("-Give the game its words") && diff.contains("+Write all text"),
        "{diff}"
    );
}

#[test]
fn non_director_agents_have_no_agent_tool() {
    let dir = project();
    write(dir.path());
    for file in agent_files(dir.path())
        .iter()
        .filter(|f| !f.ends_with("director.md"))
    {
        let text = std::fs::read_to_string(file).unwrap();
        assert!(
            !tools(&text).iter().any(|t| t.starts_with("Agent")),
            "{}",
            file.display()
        );
    }
}

#[test]
fn director_agent_tools_restrict_spawnable_roles() {
    let dir = project();
    write(dir.path());
    let director = read(dir.path(), ".claude/agents/director.md");
    assert!(
        tools(&director)
            .contains(&"Agent(level-designer, qa-playtester, tech-artist, writer)".to_owned()),
        "{director}"
    );
}

#[test]
fn agent_tools_list_matches_role_mcp_scope_exactly() {
    let dir = project();
    write(dir.path());
    let roster = common::sync::roster(dir.path());
    for role in roster.roles() {
        let text = read(dir.path(), &format!(".claude/agents/{}.md", role.role));
        let listed: BTreeSet<String> = tools(&text)
            .into_iter()
            .filter_map(|t| t.strip_prefix("mcp__concordia__").map(str::to_owned))
            .collect();
        let scope: BTreeSet<String> = role.mcp_tools.iter().cloned().collect();
        assert_eq!(listed, scope, "{}", role.role);
    }
}

#[test]
fn generated_agent_frontmatter_uses_only_documented_claude_code_fields() {
    let dir = project();
    write(dir.path());
    for file in agent_files(dir.path()) {
        for key in frontmatter_keys(&std::fs::read_to_string(&file).unwrap()) {
            assert!(
                CLAUDE_AGENT_FIELDS.contains(&key.as_str()),
                "{}: {key}",
                file.display()
            );
        }
    }
}

#[test]
fn generated_frontmatter_never_contains_hooks_mcp_servers_or_permission_mode() {
    let dir = project();
    write(dir.path());
    for file in agent_files(dir.path()) {
        let keys = frontmatter_keys(&std::fs::read_to_string(&file).unwrap());
        for banned in ["hooks", "mcpServers", "permissionMode"] {
            assert!(
                !keys.iter().any(|k| k == banned),
                "{}: {banned}",
                file.display()
            );
        }
    }
}

#[test]
fn generated_files_contain_no_secrets_or_absolute_user_paths() {
    let dir = project();
    let report = write(dir.path());
    let root = dir.path().to_string_lossy().into_owned();
    for change in &report.changes {
        let text = read(dir.path(), &change.path);
        for bad in [
            "/Users/",
            "/home/",
            "ANTHROPIC_API_KEY",
            "sk-ant-",
            root.as_str(),
        ] {
            assert!(!text.contains(bad), "{} contains {bad}", change.path);
        }
    }
}

#[test]
fn sync_output_is_independent_of_filesystem_order() {
    let a = project();
    let b = tempfile::tempdir().unwrap();
    // Same files, written in reverse order.
    let mut files: Vec<_> = walk(&a.path().join("studio"));
    files.reverse();
    for path in files {
        let relative = path.strip_prefix(a.path()).unwrap();
        let target = b.path().join(relative);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(&path, target).unwrap();
    }
    write(a.path());
    write(b.path());
    for relative in [
        ".claude/settings.json",
        ".mcp.json",
        "CLAUDE.md",
        ".claude/agents/director.md",
    ] {
        assert_eq!(
            read(a.path(), relative),
            read(b.path(), relative),
            "{relative}"
        );
    }
}

#[test]
fn hook_entries_run_concordia_with_a_short_timeout() {
    let dir = project();
    write(dir.path());
    let settings: Value = serde_json::from_str(&read(dir.path(), ".claude/settings.json")).unwrap();
    let mut count = 0;
    for groups in settings["hooks"].as_object().unwrap().values() {
        for handler in groups
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|g| g["hooks"].as_array().unwrap())
        {
            assert_eq!(handler["command"], "concordia");
            assert_eq!(handler["args"][0], "studio");
            assert!(handler["timeout"].as_u64().unwrap() <= 10, "{handler}");
            count += 1;
        }
    }
    // 7 since 3.5: SessionEnd releases the main session's slot (the live probe found it held).
    assert_eq!(
        count, 7,
        "PreToolUse ×3, PostToolUse, SubagentStart, SubagentStop, SessionEnd"
    );
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        match path.is_dir() {
            true => out.extend(walk(&path)),
            false => out.push(path),
        }
    }
    out.sort();
    out
}
