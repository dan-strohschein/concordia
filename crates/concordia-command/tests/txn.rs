//! Atomic transactions (M0 phase 0.4).

mod common;

use common::{Fixture, author, pos_x, set_pos, spawn};
use concordia_command::{CommandError, EditOp, EntryKind};
use concordia_ecs::EcsError;

#[test]
fn committed_txn_records_actor_label_and_ops() {
    let mut fx = Fixture::new();
    let before = fx.hash();
    let (a, b) = (fx.entity_id(), fx.entity_id());
    fx.log
        .begin(&fx.world, author("agent:builder"), "build", &mut fx.ids)
        .unwrap();
    for op in [spawn(a, "A", 1.0), spawn(b, "B", 2.0), set_pos(a, 3.0)] {
        fx.log.apply(&mut fx.world, &fx.reg, op).unwrap();
    }
    let txn = fx.log.commit(&mut fx.world, &fx.reg).unwrap();

    let last = fx.log.done().last().unwrap();
    assert_eq!(last.id, txn);
    assert_eq!(last.author.actor.to_string(), "agent:builder");
    assert_eq!(last.label, "build");
    assert_eq!(last.ops.len(), 3);
    assert_eq!(last.hash_before, before);
    assert_eq!(last.hash_after, fx.hash());
    let entry = fx.log.history(1).pop().unwrap();
    assert_eq!(
        (entry.kind, entry.ops, entry.label.as_str()),
        (EntryKind::Commit, 3, "build")
    );
}

#[test]
fn ops_apply_immediately_inside_a_txn() {
    let mut fx = Fixture::new();
    let a = fx.entity_id();
    fx.log
        .begin(&fx.world, author("agent:x"), "t", &mut fx.ids)
        .unwrap();
    fx.log
        .apply(&mut fx.world, &fx.reg, spawn(a, "A", 1.0))
        .unwrap();
    assert_eq!(
        pos_x(&fx.world, a),
        Some(1.0),
        "reads inside a txn see its writes"
    );
}

#[test]
fn failed_op_rolls_back_whole_txn() {
    let mut fx = Fixture::new();
    let before = fx.hash();
    let (a, ghost) = (fx.entity_id(), fx.entity_id());
    fx.log
        .begin(&fx.world, author("agent:x"), "doomed", &mut fx.ids)
        .unwrap();
    fx.log
        .apply(&mut fx.world, &fx.reg, spawn(a, "A", 1.0))
        .unwrap();
    fx.log
        .apply(&mut fx.world, &fx.reg, set_pos(a, 2.0))
        .unwrap();
    let err = fx
        .log
        .apply(&mut fx.world, &fx.reg, set_pos(ghost, 3.0))
        .unwrap_err();
    assert!(matches!(
        err,
        CommandError::OpFailed { index: 2, source: EcsError::NoSuchEntity(id) } if id == ghost
    ));
    assert_eq!(fx.hash(), before);
    assert!(fx.log.open_txn().is_none());
    assert!(fx.log.done().is_empty());
    let entry = fx.log.history(1).pop().unwrap();
    assert_eq!(
        (entry.kind, entry.reason.as_deref()),
        (EntryKind::Abort, Some("op_failed"))
    );
}

#[test]
fn invalid_value_fails_before_touching_world() {
    let mut fx = Fixture::new();
    let a = fx.entity_id();
    fx.one("agent:x", spawn(a, "A", 1.0));
    let before = fx.hash();
    let bad = EditOp::SetComponent {
        id: a,
        component: "Pos".into(),
        value: serde_json::json!({"x": "far"}),
    };
    let err = fx
        .log
        .apply_one(
            &mut fx.world,
            &fx.reg,
            author("agent:x"),
            "set",
            bad,
            &mut fx.ids,
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            CommandError::OpFailed {
                index: 0,
                source: EcsError::Reflect(_)
            }
        ),
        "{err:?}"
    );
    assert_eq!(fx.hash(), before);
    let bad_spawn = EditOp::Spawn {
        id: fx.entity_id(),
        components: [("Pos".to_owned(), serde_json::json!(1))].into(),
    };
    assert!(
        fx.log
            .apply_one(
                &mut fx.world,
                &fx.reg,
                author("agent:x"),
                "s",
                bad_spawn,
                &mut fx.ids
            )
            .is_err()
    );
    assert_eq!(fx.hash(), before, "a failed spawn leaves no entity behind");
}

#[test]
fn abort_restores_world() {
    let mut fx = Fixture::new();
    let before = fx.hash();
    let a = fx.entity_id();
    fx.log
        .begin(&fx.world, author("agent:x"), "maybe", &mut fx.ids)
        .unwrap();
    fx.log
        .apply(&mut fx.world, &fx.reg, spawn(a, "A", 1.0))
        .unwrap();
    fx.log
        .apply(&mut fx.world, &fx.reg, set_pos(a, 9.0))
        .unwrap();
    fx.log.abort(&mut fx.world, &fx.reg, "aborted").unwrap();
    assert_eq!(fx.hash(), before);
    assert!(fx.log.done().is_empty());
    let entry = fx.log.history(1).pop().unwrap();
    assert_eq!(
        (entry.kind, entry.reason.as_deref()),
        (EntryKind::Abort, Some("aborted"))
    );
}

#[test]
fn implicit_single_op_txn_when_none_open() {
    let mut fx = Fixture::new();
    let a = fx.entity_id();
    let txn = fx.one("agent:x", spawn(a, "A", 1.0));
    let last = fx.log.done().last().unwrap();
    assert_eq!(
        (last.id, last.label.as_str(), last.ops.len()),
        (txn, "spawn", 1)
    );
    assert!(fx.log.open_txn().is_none());
}

#[test]
fn nested_begin_is_rejected() {
    let mut fx = Fixture::new();
    let txn = fx
        .log
        .begin(&fx.world, author("agent:a"), "first", &mut fx.ids)
        .unwrap();
    let err = fx
        .log
        .begin(&fx.world, author("agent:b"), "second", &mut fx.ids)
        .unwrap_err();
    assert!(matches!(
        err,
        CommandError::AlreadyOpen { txn: open, ref actor, ref label } if open == txn && actor.to_string() == "agent:a" && label == "first"
    ));
    let id = fx.entity_id();
    assert!(matches!(
        fx.log.apply_one(
            &mut fx.world,
            &fx.reg,
            author("agent:b"),
            "x",
            spawn(id, "X", 0.0),
            &mut fx.ids
        ),
        Err(CommandError::AlreadyOpen { .. })
    ));
}

#[test]
fn apply_and_commit_without_open_txn_fail() {
    let mut fx = Fixture::new();
    let a = fx.entity_id();
    assert!(matches!(
        fx.log.apply(&mut fx.world, &fx.reg, spawn(a, "A", 0.0)),
        Err(CommandError::NoOpenTxn)
    ));
    assert!(matches!(
        fx.log.commit(&mut fx.world, &fx.reg),
        Err(CommandError::NoOpenTxn)
    ));
    assert!(matches!(
        fx.log.abort(&mut fx.world, &fx.reg, "x"),
        Err(CommandError::NoOpenTxn)
    ));
}

#[test]
fn remove_missing_component_is_an_error() {
    let mut fx = Fixture::new();
    let a = fx.entity_id();
    fx.one("agent:x", spawn(a, "A", 0.0));
    let op = EditOp::RemoveComponent {
        id: a,
        component: "Nope".into(),
    };
    let err = fx
        .log
        .apply_one(
            &mut fx.world,
            &fx.reg,
            author("agent:x"),
            "rm",
            op,
            &mut fx.ids,
        )
        .unwrap_err();
    assert!(matches!(
        err,
        CommandError::OpFailed {
            source: EcsError::ComponentMissing { .. },
            ..
        }
    ));
}
