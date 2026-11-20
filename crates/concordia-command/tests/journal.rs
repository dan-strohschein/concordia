//! Persisted JSON Lines journal (M0 phase 0.4).

mod common;

use std::io::Write;

use common::{Fixture, author, set_pos, spawn};
use concordia_command::{Journal, JournalConfig, JournalWarning};
use serde_json::Value;

fn journal_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
    dir.path().join(".concordia/journal/scene.cmdlog.jsonl")
}

fn lines(path: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn fixture_with_journal(path: &std::path::Path) -> Fixture {
    let mut fx = Fixture::new();
    let (journal, loaded) = Journal::open(path, JournalConfig::default()).unwrap();
    assert!(loaded.entries.is_empty());
    fx.log.attach_journal(journal);
    fx
}

#[test]
fn journal_appends_one_json_line_per_event() {
    let dir = tempfile::tempdir().unwrap();
    let path = journal_path(&dir);
    let mut fx = fixture_with_journal(&path);
    let a = fx.entity_id();
    fx.one("agent:x", spawn(a, "A", 1.0));
    fx.log
        .undo(&mut fx.world, &mut fx.reg, author("agent:x"))
        .unwrap();
    fx.log
        .redo(&mut fx.world, &mut fx.reg, author("agent:x"))
        .unwrap();
    fx.log
        .begin(&fx.world, author("agent:x"), "nope", &mut fx.ids)
        .unwrap();
    fx.log.abort(&mut fx.world, &mut fx.reg, "aborted").unwrap();
    let hash = fx.hash();
    fx.log
        .record_save(author("agent:x"), "scenes/main.scene.ron", hash)
        .unwrap();

    let entries = lines(&path);
    let kinds: Vec<&str> = entries
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["commit", "undo", "redo", "abort", "save"]);
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(entry["v"], 1);
        assert_eq!(entry["seq"], i as u64 + 1);
        assert!(entry["ts_ms"].as_u64().unwrap() > 0);
    }
}

#[test]
fn journal_tolerates_truncated_last_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = journal_path(&dir);
    {
        let mut fx = fixture_with_journal(&path);
        let a = fx.entity_id();
        fx.one("agent:x", spawn(a, "A", 1.0));
        fx.one("agent:x", set_pos(a, 2.0));
    }
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    file.write_all(br#"{"v":1,"seq":3,"kind":"comm"#).unwrap();
    drop(file);

    let loaded = Journal::load(&path).unwrap();
    assert_eq!(loaded.entries.len(), 2);
    assert_eq!(
        loaded.warnings,
        vec![JournalWarning::TruncatedTail { line: 3 }]
    );

    let (journal, reopened) = Journal::open(&path, JournalConfig::default()).unwrap();
    assert_eq!(reopened.entries.len(), 2);
    drop(journal);
    assert_eq!(lines(&path).len(), 2, "garbage tail is cut on reopen");
}

#[test]
fn journal_rejects_unknown_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("j.cmdlog.jsonl");
    std::fs::write(&path, "{\"v\":2,\"seq\":1,\"kind\":\"save\"}\n").unwrap();
    assert!(Journal::load(&path).is_err());
}

#[test]
fn journal_entry_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = journal_path(&dir);
    let mut fx = fixture_with_journal(&path);
    let a = fx.entity_id();
    fx.one("agent:claude-code", spawn(a, "Player", 5.0));
    let entry = lines(&path).remove(0);
    insta::assert_json_snapshot!(entry, { ".ts_ms" => "[ts]" });
}
