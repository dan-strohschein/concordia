//! Linear undo and redo (M0 phase 0.4).

mod common;

use common::{Fixture, author, json_of, pos_x, set_pos, spawn};
use concordia_command::{CommandError, EditOp, EntryKind};
use proptest::prelude::*;

#[test]
fn undo_restores_previous_component_value() {
    let mut fx = Fixture::new();
    let a = fx.entity_id();
    fx.one("agent:x", spawn(a, "A", 1.0));
    fx.one("agent:x", set_pos(a, 5.0));
    fx.log
        .undo(&mut fx.world, &fx.reg, author("agent:x"))
        .unwrap();
    assert_eq!(pos_x(&fx.world, a), Some(1.0));
}

#[test]
fn undo_of_set_on_missing_component_removes_it() {
    let mut fx = Fixture::new();
    let a = fx.entity_id();
    fx.one(
        "agent:x",
        EditOp::Spawn {
            id: a,
            components: Default::default(),
        },
    );
    fx.one("agent:x", set_pos(a, 5.0));
    fx.log
        .undo(&mut fx.world, &fx.reg, author("agent:x"))
        .unwrap();
    assert_eq!(fx.world.component_names(a), Some(vec![]));
}

#[test]
fn undo_of_spawn_despawns_and_redo_respawns_same_id() {
    let mut fx = Fixture::new();
    let a = fx.entity_id();
    fx.one("agent:x", spawn(a, "A", 1.0));
    fx.log
        .undo(&mut fx.world, &fx.reg, author("agent:x"))
        .unwrap();
    assert!(!fx.world.contains(a));
    fx.log
        .redo(&mut fx.world, &fx.reg, author("agent:x"))
        .unwrap();
    assert_eq!(pos_x(&fx.world, a), Some(1.0));
}

#[test]
fn undo_of_despawn_restores_all_components_and_same_id() {
    let mut fx = Fixture::new();
    let a = fx.entity_id();
    fx.one("agent:x", spawn(a, "Crate", 3.25));
    let before = (
        json_of(&fx.world, a, "Name"),
        json_of(&fx.world, a, "Pos"),
        fx.hash(),
    );
    fx.one("agent:x", EditOp::Despawn { id: a });
    fx.log
        .undo(&mut fx.world, &fx.reg, author("agent:x"))
        .unwrap();
    let after = (
        json_of(&fx.world, a, "Name"),
        json_of(&fx.world, a, "Pos"),
        fx.hash(),
    );
    assert_eq!(before, after);
}

#[test]
fn redo_stack_cleared_by_new_commit() {
    let mut fx = Fixture::new();
    let a = fx.entity_id();
    fx.one("agent:x", spawn(a, "A", 1.0));
    fx.one("agent:x", set_pos(a, 2.0));
    fx.log
        .undo(&mut fx.world, &fx.reg, author("agent:x"))
        .unwrap();
    assert!(fx.log.can_redo());
    fx.one("agent:x", set_pos(a, 7.0));
    assert!(!fx.log.can_redo());
    assert!(matches!(
        fx.log.redo(&mut fx.world, &fx.reg, author("agent:x")),
        Err(CommandError::NothingToRedo)
    ));
}

#[test]
fn undo_redo_are_recorded_with_actor() {
    let mut fx = Fixture::new();
    let a = fx.entity_id();
    let txn = fx.one("agent:maker", spawn(a, "A", 1.0));
    fx.log
        .undo(&mut fx.world, &fx.reg, author("agent:reviewer"))
        .unwrap();
    fx.log
        .redo(&mut fx.world, &fx.reg, author("human:local"))
        .unwrap();
    let history = fx.log.history(10);
    let kinds: Vec<_> = history
        .iter()
        .map(|e| (e.kind, e.actor.to_string(), e.target))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (EntryKind::Commit, "agent:maker".into(), None),
            (EntryKind::Undo, "agent:reviewer".into(), Some(txn)),
            (EntryKind::Redo, "human:local".into(), Some(txn)),
        ]
    );
}

#[test]
fn nothing_to_undo_is_an_error_with_suggestion() {
    let mut fx = Fixture::new();
    let err = fx
        .log
        .undo(&mut fx.world, &fx.reg, author("agent:x"))
        .unwrap_err();
    let CommandError::NothingToUndo { reason } = &err else {
        panic!("{err:?}")
    };
    assert!(reason.contains("txn.history"), "{reason}");
}

#[test]
fn undo_is_rejected_while_a_txn_is_open() {
    let mut fx = Fixture::new();
    let a = fx.entity_id();
    fx.one("agent:x", spawn(a, "A", 1.0));
    fx.log
        .begin(&fx.world, author("agent:y"), "open", &mut fx.ids)
        .unwrap();
    assert!(matches!(
        fx.log.undo(&mut fx.world, &fx.reg, author("agent:x")),
        Err(CommandError::AlreadyOpen { .. })
    ));
}

#[derive(Debug, Clone)]
enum Step {
    Spawn,
    Set(usize, f32),
    Despawn(usize),
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        Just(Step::Spawn),
        (0..8usize, -100.0f32..100.0).prop_map(|(i, x)| Step::Set(i, x)),
        (0..8usize).prop_map(Step::Despawn),
    ]
}

proptest! {
    #[test]
    fn undo_then_redo_hash_roundtrip_proptest(steps in prop::collection::vec(step(), 1..40)) {
        let mut fx = Fixture::new();
        let initial = fx.hash();
        let mut alive = Vec::new();
        let mut commits = 0;
        for step in steps {
            let op = match step {
                Step::Spawn => {
                    let id = fx.entity_id();
                    alive.push(id);
                    spawn(id, "e", 0.0)
                }
                Step::Set(i, x) if !alive.is_empty() => set_pos(alive[i % alive.len()], x),
                Step::Despawn(i) if !alive.is_empty() => EditOp::Despawn { id: alive.remove(i % alive.len()) },
                _ => continue,
            };
            fx.one("agent:p", op);
            commits += 1;
        }
        let last = fx.hash();
        for _ in 0..commits {
            fx.log.undo(&mut fx.world, &fx.reg, author("agent:p")).unwrap();
        }
        prop_assert_eq!(fx.hash(), initial);
        for _ in 0..commits {
            fx.log.redo(&mut fx.world, &fx.reg, author("agent:p")).unwrap();
        }
        prop_assert_eq!(fx.hash(), last);
        prop_assert_eq!(fx.log.history(1000).iter().filter(|e| e.kind == EntryKind::Commit).count(), commits);
    }
}
