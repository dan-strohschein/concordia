//! Document edits (M4 4.10c, J17, D9): a text file changed as an undoable op with a byte-exact
//! undo, journaled with both texts, refused when the file changed behind the log's back.

mod common;

use std::sync::Arc;

use common::{Fixture, author};
use concordia_command::{
    CommandError, Documents, EditOp, Journal, JournalConfig, MemoryDocuments, ProjectDocuments,
    replay_journal,
};

const PATH: &str = "graphs/enemy.graph.ron";
/// Odd bytes on purpose: CRLF, trailing spaces, no final newline, non-ASCII.
const OLD: &str = "Graph(\r\n  id: \"a\",   \n)\t ünïcode";
const NEW: &str = "Graph(\n    id: \"b\",\n)\n";

fn edit(before: Option<&str>, after: Option<&str>) -> EditOp {
    EditOp::Document {
        path: PATH.into(),
        before: before.map(str::to_owned),
        after: after.map(str::to_owned),
    }
}

fn fixture(docs: &Arc<MemoryDocuments>) -> Fixture {
    let mut fx = Fixture::new();
    fx.log.attach_documents(docs.clone());
    fx
}

#[test]
fn graph_apply_is_a_transaction_with_actor_and_undoable() {
    let docs = Arc::new(MemoryDocuments::default());
    docs.write(PATH, Some(OLD)).unwrap();
    let mut fx = fixture(&docs);
    let hash = fx.hash();
    fx.one("agent:graphs", edit(Some(OLD), Some(NEW)));
    assert_eq!(docs.read(PATH).unwrap().as_deref(), Some(NEW));
    assert_eq!(fx.hash(), hash, "documents don't change the world");
    let history = fx.log.history(10);
    assert_eq!(history.last().unwrap().actor.to_string(), "agent:graphs");
    fx.log
        .undo(&mut fx.world, &mut fx.reg, author("agent:graphs"))
        .unwrap();
    assert_eq!(
        docs.read(PATH).unwrap().as_deref(),
        Some(OLD),
        "byte for byte"
    );
    fx.log
        .redo(&mut fx.world, &mut fx.reg, author("agent:graphs"))
        .unwrap();
    assert_eq!(docs.read(PATH).unwrap().as_deref(), Some(NEW));
}

#[test]
fn creating_a_document_undoes_to_absent() {
    let docs = Arc::new(MemoryDocuments::default());
    let mut fx = fixture(&docs);
    fx.one("agent:graphs", edit(None, Some(NEW)));
    assert_eq!(docs.read(PATH).unwrap().as_deref(), Some(NEW));
    fx.log
        .undo(&mut fx.world, &mut fx.reg, author("agent:graphs"))
        .unwrap();
    assert_eq!(docs.read(PATH).unwrap(), None);
}

#[test]
fn an_edit_behind_the_logs_back_is_a_conflict() {
    let docs = Arc::new(MemoryDocuments::default());
    docs.write(PATH, Some("someone else's text")).unwrap();
    let mut fx = fixture(&docs);
    let err = fx
        .log
        .apply_one(
            &mut fx.world,
            &mut fx.reg,
            author("agent:graphs"),
            "document",
            edit(Some(OLD), Some(NEW)),
            &mut fx.ids,
        )
        .unwrap_err();
    assert!(matches!(err, CommandError::Document { .. }), "{err}");
    assert!(err.to_string().contains("changed"), "{err}");
    assert_eq!(
        docs.read(PATH).unwrap().as_deref(),
        Some("someone else's text")
    );
    assert!(fx.log.history(10).is_empty(), "a failed op leaves no trace");
}

#[test]
fn document_paths_stay_inside_the_project() {
    let dir = tempfile::tempdir().unwrap();
    let docs = ProjectDocuments::new(dir.path());
    for bad in [
        "",
        "../outside.txt",
        "/etc/passwd",
        "graphs/../../x",
        "a\\b",
    ] {
        assert!(docs.read(bad).is_err(), "{bad:?} must be refused");
        assert!(
            docs.write(bad, Some("x")).is_err(),
            "{bad:?} must be refused"
        );
    }
    docs.write("graphs/new/enemy.graph.ron", Some(NEW)).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("graphs/new/enemy.graph.ron")).unwrap(),
        NEW
    );
    docs.write("graphs/new/enemy.graph.ron", None).unwrap();
    assert!(!dir.path().join("graphs/new/enemy.graph.ron").exists());
}

#[test]
fn document_ops_need_documents_attached() {
    let mut fx = Fixture::new();
    let err = fx
        .log
        .apply_one(
            &mut fx.world,
            &mut fx.reg,
            author("agent:graphs"),
            "document",
            edit(None, Some(NEW)),
            &mut fx.ids,
        )
        .unwrap_err();
    assert!(matches!(err, CommandError::Document { .. }), "{err}");
}

#[test]
fn document_edits_journal_both_texts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".concordia/journal/scene.cmdlog.jsonl");
    let docs = Arc::new(MemoryDocuments::default());
    docs.write(PATH, Some(OLD)).unwrap();
    let mut fx = fixture(&docs);
    let (journal, _) = Journal::open(&path, JournalConfig::default()).unwrap();
    fx.log.attach_journal(journal);
    fx.one("agent:graphs", edit(Some(OLD), Some(NEW)));
    let text = std::fs::read_to_string(&path).unwrap();
    let line: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    let flat = line.to_string();
    assert!(
        flat.contains(&serde_json::to_string(OLD).unwrap()),
        "{flat}"
    );
    assert!(
        flat.contains(&serde_json::to_string(NEW).unwrap()),
        "{flat}"
    );
    // Replay rebuilds the world without touching files (they already hold the final text).
    let (_, loaded) = Journal::open(&path, JournalConfig::default()).unwrap();
    let mut fresh = Fixture::new();
    replay_journal(&loaded.entries, &mut fresh.world, &mut fresh.reg).unwrap();
    assert_eq!(fresh.hash(), fx.hash());
}
