//! The Studio journal: `.concordia/studio/journal.jsonl` (ADR-0080, M3 phase 3.3).

use concordia_command::Actor;
use concordia_studio::{JournalEvent, StudioJournal};

fn actor(text: &str) -> Actor {
    text.parse().unwrap()
}

fn comment(task: &str, text: &str) -> JournalEvent {
    JournalEvent::TaskComment {
        task: task.into(),
        text: text.into(),
    }
}

#[test]
fn journal_appends_are_versioned_lines_with_actor_and_time() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".concordia/studio/journal.jsonl");
    let journal = StudioJournal::new(&path);
    journal
        .append(&actor("agent:director"), comment("T-0001", "hello"))
        .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let line: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    assert_eq!(line["v"], 1);
    assert_eq!(line["actor"], "agent:director");
    assert_eq!(line["kind"], "task_comment");
    assert!(line["ts_ms"].as_u64().unwrap() > 0);
}

#[test]
fn seq_is_line_order_and_since_skips_earlier_entries() {
    let dir = tempfile::tempdir().unwrap();
    let journal = StudioJournal::new(dir.path().join("j.jsonl"));
    for i in 0..5 {
        journal
            .append(&actor("agent:director"), comment("T-0001", &i.to_string()))
            .unwrap();
    }
    let all = journal.read(None).unwrap();
    assert_eq!(
        all.iter().map(|e| e.seq).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5]
    );
    assert_eq!(journal.read(Some(3)).unwrap().len(), 2);
}

#[test]
fn concurrent_appends_never_interleave() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("j.jsonl");
    let long = "x".repeat(10_000);
    let workers: Vec<_> = (0..8)
        .map(|w| {
            let (path, long) = (path.clone(), long.clone());
            std::thread::spawn(move || {
                let journal = StudioJournal::new(path);
                for _ in 0..25 {
                    journal
                        .append(&actor(&format!("agent:w{w}")), comment("T-0001", &long))
                        .unwrap();
                }
            })
        })
        .collect();
    workers.into_iter().for_each(|w| w.join().unwrap());
    assert_eq!(StudioJournal::new(&path).read(None).unwrap().len(), 200);
}

#[test]
fn truncated_tail_is_ignored_and_missing_file_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("j.jsonl");
    let journal = StudioJournal::new(&path);
    assert!(journal.read(None).unwrap().is_empty());
    journal
        .append(&actor("agent:director"), comment("T-0001", "ok"))
        .unwrap();
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("{\"v\":1,\"actor\":");
    std::fs::write(&path, text).unwrap();
    assert_eq!(journal.read(None).unwrap().len(), 1);
    journal
        .append(&actor("agent:director"), comment("T-0001", "after"))
        .unwrap();
    assert_eq!(
        journal.read(None).unwrap().len(),
        2,
        "a later append starts on a fresh line"
    );
}

#[test]
fn newer_journal_version_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("j.jsonl");
    std::fs::write(
        &path,
        "{\"v\":9,\"ts_ms\":1,\"actor\":\"agent:a\",\"kind\":\"sync\"}\n",
    )
    .unwrap();
    assert!(StudioJournal::new(&path).read(None).is_err());
}
