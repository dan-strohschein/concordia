//! Journal replay and history resume (M0 phase 0.4).

mod common;

use common::{Fixture, author, pos_x, set_pos, spawn};
use concordia_command::{
    CommandError, CommandLog, EditOp, History, Journal, JournalConfig, replay_journal,
};

fn recorded_session(path: &std::path::Path) -> (Fixture, concordia_terra::World) {
    let mut fx = Fixture::new();
    let base = fx.world.clone();
    let (journal, _) = Journal::open(path, JournalConfig::default()).unwrap();
    fx.log.attach_journal(journal);
    let (a, b) = (fx.entity_id(), fx.entity_id());
    fx.one("agent:x", spawn(a, "A", 1.0));
    fx.one("agent:x", spawn(b, "B", 2.0));
    fx.one("agent:x", set_pos(a, 3.0));
    fx.log
        .undo(&mut fx.world, &fx.reg, author("agent:x"))
        .unwrap();
    fx.one("agent:x", EditOp::Despawn { id: b });
    fx.log
        .undo(&mut fx.world, &fx.reg, author("agent:x"))
        .unwrap();
    fx.log
        .redo(&mut fx.world, &fx.reg, author("agent:x"))
        .unwrap();
    (fx, base)
}

#[test]
fn replaying_journal_from_base_snapshot_reproduces_hash() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.cmdlog.jsonl");
    let (fx, mut base) = recorded_session(&path);
    let entries = Journal::load(&path).unwrap().entries;
    let final_hash = replay_journal(&entries, &mut base, &fx.reg).unwrap();
    assert_eq!(final_hash, Some(fx.hash()));
    assert_eq!(base.state_hash().unwrap(), fx.hash());
}

#[test]
fn replay_detects_divergence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.cmdlog.jsonl");
    let (mut fx, _) = recorded_session(&path);
    let entries = Journal::load(&path).unwrap().entries;
    let mut other = Fixture::new();
    let stray = fx.entity_id();
    other.one("agent:y", spawn(stray, "stray", 0.0));
    let err = replay_journal(&entries, &mut other.world, &fx.reg).unwrap_err();
    assert!(
        matches!(err, CommandError::Diverged { seq: 1, .. }),
        "{err:?}"
    );
}

#[test]
fn history_reloads_when_scene_hash_matches() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.cmdlog.jsonl");
    let (fx, _) = recorded_session(&path);
    let mut world = fx.world.clone();
    let entries = Journal::load(&path).unwrap().entries;

    let (mut log, history) = CommandLog::resume(&entries, world.state_hash().unwrap());
    assert_eq!(history, History::Restored);
    let a = entries_first_spawn(&entries);
    log.undo(&mut world, &fx.reg, author("agent:z")).unwrap();
    log.undo(&mut world, &fx.reg, author("agent:z")).unwrap();
    assert!(world.contains(a) && pos_x(&world, a) == Some(1.0));
    assert!(log.can_redo(), "the two undos are redoable");
    let entry = log.history(1).pop().unwrap();
    assert_eq!(
        entry.seq,
        entries.last().unwrap().seq + 2,
        "seq continues after the journal"
    );

    let (mut stale, history) = CommandLog::resume(
        &entries,
        concordia_terra::World::new().state_hash().unwrap(),
    );
    assert!(matches!(history, History::Stale { .. }));
    let err = stale
        .undo(&mut world, &fx.reg, author("agent:z"))
        .unwrap_err();
    let CommandError::NothingToUndo { reason } = err else {
        panic!()
    };
    assert!(reason.contains("stale"), "{reason}");

    let (_, none) = CommandLog::resume(&[], world.state_hash().unwrap());
    assert_eq!(none, History::None);
}

fn entries_first_spawn(
    entries: &[concordia_command::JournalEntry],
) -> concordia_core::id::EntityId {
    entries
        .iter()
        .find_map(|e| match &e.event {
            concordia_command::Event::Commit { ops, .. } => match &ops[0].op {
                EditOp::Spawn { id, .. } => Some(*id),
                _ => None,
            },
            _ => None,
        })
        .unwrap()
}
