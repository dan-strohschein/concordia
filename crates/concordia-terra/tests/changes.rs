//! Change tracking (M1 phase 1.9e): which entities changed since a tick, so consumers such as the
//! renderer update incrementally instead of re-reading the whole world.

mod common;

use common::{A, B, Pos, Vel, ids, moving_world, registry};
use concordia_terra::{QueryState, World};

fn sorted(mut v: Vec<concordia_core::id::EntityId>) -> Vec<concordia_core::id::EntityId> {
    v.sort();
    v
}

#[test]
fn writes_mark_only_the_entities_they_touch() {
    let mut world = World::new();
    let [a, b, c] = ids(3).try_into().unwrap();
    for id in [a, b, c] {
        world.spawn_with_id(id).unwrap();
        world.insert(id, A(0)).unwrap();
    }
    let t0 = world.change_tick();
    assert!(
        world.changes_since(t0).unwrap().changed.is_empty(),
        "reading changes nothing"
    );
    let _ = world.get::<A>(a);
    world.insert(b, A(1)).unwrap();
    assert_eq!(world.changes_since(t0).unwrap().changed, vec![b]);
    let t1 = world.change_tick();
    *world.get_mut::<A>(c).unwrap() = A(2);
    world.insert(a, B(1.0)).unwrap();
    assert_eq!(
        sorted(world.changes_since(t1).unwrap().changed),
        sorted(vec![a, c])
    );
    let t2 = world.change_tick();
    world.remove::<B>(a).unwrap();
    world
        .set_json(b, "A", serde_json::json!(5), &registry())
        .unwrap();
    assert_eq!(
        sorted(world.changes_since(t2).unwrap().changed),
        sorted(vec![a, b])
    );
}

#[test]
fn spawns_and_despawns_are_reported() {
    let mut world = World::new();
    let [a, b] = ids(2).try_into().unwrap();
    world.spawn_with_id(a).unwrap();
    let t0 = world.change_tick();
    world.spawn_with_id(b).unwrap();
    world.despawn(a).unwrap();
    let changes = world.changes_since(t0).unwrap();
    assert_eq!(changes.changed, vec![b]);
    assert_eq!(changes.despawned, vec![a]);
}

#[test]
fn mutable_queries_mark_what_they_visit_and_read_only_ones_do_not() {
    let mut world = moving_world(4);
    let t0 = world.change_tick();
    let mut read = QueryState::<(&Pos,)>::new().unwrap();
    read.for_each(&mut world, |_, _| {});
    assert!(world.changes_since(t0).unwrap().changed.is_empty());
    let mut write = QueryState::<(&mut Pos, &Vel)>::new().unwrap();
    write.for_each(&mut world, |_, (p, v)| p.x += v.x);
    assert_eq!(world.changes_since(t0).unwrap().changed.len(), 4);
}

#[test]
fn a_tick_older_than_the_despawn_history_asks_for_a_full_reread() {
    let mut world = World::new();
    let t0 = world.change_tick();
    for id in ids(World::DESPAWN_HISTORY + 1) {
        world.spawn_with_id(id).unwrap();
        world.despawn(id).unwrap();
    }
    assert!(world.changes_since(t0).is_none());
    assert!(world.changes_since(world.change_tick()).is_some());
}

#[test]
fn joining_a_written_archetype_later_does_not_inherit_the_earlier_write() {
    let mut world = moving_world(2);
    let late = ids(3)[2];
    world.spawn_with_id(late).unwrap();
    world.insert(late, Vel::default()).unwrap();
    let t0 = world.change_tick();
    let mut write = QueryState::<(&mut Pos, &Vel)>::new().unwrap();
    write.for_each(&mut world, |_, (p, v)| p.x += v.x);
    let t1 = world.change_tick();
    world.insert(late, Pos::default()).unwrap();
    assert_eq!(world.changes_since(t1).unwrap().changed, vec![late]);
    assert_eq!(world.changes_since(t0).unwrap().changed.len(), 3);
}

#[test]
fn leaving_a_written_archetype_keeps_the_write() {
    let mut world = moving_world(2);
    let moved = world.entity_ids_sorted()[0];
    let t0 = world.change_tick();
    let mut write = QueryState::<(&mut Pos, &Vel)>::new().unwrap();
    write.for_each(&mut world, |_, (p, v)| p.x += v.x);
    let t1 = world.change_tick();
    world.remove::<Vel>(moved).unwrap();
    assert_eq!(world.changes_since(t1).unwrap().changed, vec![moved]);
    assert_eq!(world.changes_since(t0).unwrap().changed.len(), 2);
}
