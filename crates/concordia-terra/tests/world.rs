//! World storage: spawn, despawn, components, handles, resources (M0 phase 0.3).

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};

use common::{A, Aligned, B, C, Marker, ids};
use concordia_core::id::DeterministicIdGen;
use concordia_reflect::Reflect;
use concordia_terra::{EcsError, World};
use serde::{Deserialize, Serialize};

#[test]
fn spawned_entity_has_stable_uuid() {
    let mut world = World::new();
    let [keep, other, later] = ids(3).try_into().unwrap();
    world.spawn_with_id(keep).unwrap();
    world.spawn_with_id(other).unwrap();
    world.insert(keep, A(1)).unwrap();
    world.despawn(other).unwrap();
    world.spawn_with_id(later).unwrap();
    assert!(world.contains(keep));
    assert_eq!(world.get::<A>(keep), Some(&A(1)));
    assert_eq!(world.entity_ids_sorted(), vec![keep, later]);
}

#[test]
fn spawn_assigns_ids_from_source() {
    let mut world = World::new();
    let mut source = DeterministicIdGen::new(9);
    let a = world.spawn(&mut source).unwrap();
    let b = world.spawn(&mut source).unwrap();
    assert!(a < b);
    assert_eq!(world.len(), 2);
}

#[test]
fn spawn_with_duplicate_id_fails() {
    let mut world = World::new();
    let id = ids(1)[0];
    world.spawn_with_id(id).unwrap();
    assert_eq!(
        world.spawn_with_id(id).unwrap_err(),
        EcsError::DuplicateId(id)
    );
}

#[test]
fn insert_moves_entity_between_archetypes_preserving_other_components() {
    let mut world = World::new();
    let id = ids(1)[0];
    world
        .spawn_with_id(id)
        .unwrap()
        .insert(A(7))
        .unwrap()
        .insert(B(2.5))
        .unwrap();
    let before = world.archetype_count();
    world.insert(id, C("hi".into())).unwrap();
    assert_eq!(world.get::<A>(id), Some(&A(7)));
    assert_eq!(world.get::<B>(id), Some(&B(2.5)));
    assert_eq!(world.get::<C>(id), Some(&C("hi".into())));
    assert_eq!(world.archetype_count(), before + 1);
    assert_eq!(
        world.insert(id, A(8)).unwrap(),
        Some(A(7)),
        "replace returns old value"
    );
    world.validate_storage().unwrap();
}

#[test]
fn remove_last_component_keeps_entity_alive() {
    let mut world = World::new();
    let id = ids(1)[0];
    world.spawn_with_id(id).unwrap().insert(A(1)).unwrap();
    assert_eq!(world.remove::<A>(id).unwrap(), Some(A(1)));
    assert_eq!(world.remove::<A>(id).unwrap(), None);
    assert!(world.contains(id));
    assert_eq!(world.component_names(id), Some(vec![]));
}

#[test]
fn operations_on_missing_entity_are_errors() {
    let mut world = World::new();
    let id = ids(1)[0];
    assert_eq!(
        world.insert(id, A(1)).unwrap_err(),
        EcsError::NoSuchEntity(id)
    );
    assert_eq!(
        world.remove::<A>(id).unwrap_err(),
        EcsError::NoSuchEntity(id)
    );
    assert_eq!(world.despawn(id).unwrap_err(), EcsError::NoSuchEntity(id));
    assert_eq!(world.get::<A>(id), None);
}

static DROPS: AtomicUsize = AtomicUsize::new(0);

/// Counts drops.
#[derive(Debug, PartialEq, Serialize, Deserialize, Reflect)]
struct Dropper(u32);

impl Clone for Dropper {
    fn clone(&self) -> Self {
        Self(self.0)
    }
}

impl Drop for Dropper {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn despawn_drops_components_exactly_once() {
    let ids = ids(10);
    let mut world = World::new();
    for (i, &id) in ids.iter().enumerate() {
        world
            .spawn_with_id(id)
            .unwrap()
            .insert(Dropper(i as u32))
            .unwrap();
    }
    for &id in &ids[..5] {
        world.insert(id, A(0)).unwrap();
    }
    world.remove::<A>(ids[0]).unwrap();
    assert_eq!(
        DROPS.load(Ordering::SeqCst),
        0,
        "archetype moves never drop"
    );
    for &id in &ids[3..7] {
        world.despawn(id).unwrap();
    }
    assert_eq!(DROPS.load(Ordering::SeqCst), 4);
    drop(world.remove::<Dropper>(ids[0]).unwrap());
    assert_eq!(DROPS.load(Ordering::SeqCst), 5);
    drop(world);
    assert_eq!(DROPS.load(Ordering::SeqCst), 10);
}

#[test]
fn stale_handle_detected_by_generation() {
    let mut world = World::new();
    let [first, second] = ids(2).try_into().unwrap();
    world.spawn_with_id(first).unwrap();
    let handle = world.entity(first).unwrap();
    assert_eq!(world.id_of(handle), Some(first));
    world.despawn(first).unwrap();
    world.spawn_with_id(second).unwrap();
    assert_eq!(world.id_of(handle), None, "slot reuse bumps the generation");
    assert_eq!(world.entity(first), None);
}

#[test]
fn zero_sized_and_high_alignment_components_work() {
    let mut world = World::new();
    let ids = ids(3);
    for &id in &ids {
        world
            .spawn_with_id(id)
            .unwrap()
            .insert(Marker)
            .unwrap()
            .insert(Aligned { v: 9 })
            .unwrap();
    }
    world.despawn(ids[1]).unwrap();
    for &id in [ids[0], ids[2]].iter() {
        let aligned = world.get::<Aligned>(id).unwrap();
        assert_eq!((aligned as *const Aligned as usize) % 64, 0);
        assert_eq!(aligned.v, 9);
        assert_eq!(world.get::<Marker>(id), Some(&Marker));
    }
}

#[test]
fn get_mut_edits_in_place() {
    let mut world = World::new();
    let id = ids(1)[0];
    world.spawn_with_id(id).unwrap().insert(A(1)).unwrap();
    world.get_mut::<A>(id).unwrap().0 = 5;
    assert_eq!(world.get::<A>(id), Some(&A(5)));
}

#[test]
fn component_names_are_sorted_stable_names() {
    let mut world = World::new();
    let id = ids(1)[0];
    world
        .spawn_with_id(id)
        .unwrap()
        .insert(C("x".into()))
        .unwrap()
        .insert(A(1))
        .unwrap();
    assert_eq!(world.component_names(id), Some(vec!["A", "C"]));
}

#[derive(Debug, Clone, PartialEq)]
struct Score(u32);

#[test]
fn resources_insert_get_replace_remove() {
    let mut world = World::new();
    assert_eq!(world.insert_resource(Score(1)), None);
    assert_eq!(world.resource::<Score>(), Some(&Score(1)));
    world.resource_mut::<Score>().unwrap().0 = 2;
    assert_eq!(world.insert_resource(Score(3)), Some(Score(2)));
    let doubled = world.resource_scope(|w: &mut World, s: &mut Score| {
        assert!(
            w.resource::<Score>().is_none(),
            "taken out during the scope"
        );
        s.0 *= 2;
        s.0
    });
    assert_eq!(doubled, Some(6));
    assert_eq!(world.remove_resource::<Score>(), Some(Score(6)));
    assert_eq!(world.resource::<Score>(), None);
}

#[test]
fn cloned_world_is_independent() {
    let mut world = World::new();
    let id = ids(1)[0];
    world.spawn_with_id(id).unwrap().insert(A(1)).unwrap();
    world.insert_resource(Score(4));
    let mut play = world.clone();
    play.get_mut::<A>(id).unwrap().0 = 99;
    assert_eq!(world.get::<A>(id), Some(&A(1)));
    assert_eq!(play.resource::<Score>(), Some(&Score(4)));
    assert_eq!(
        world.state_hash().unwrap(),
        world.clone().state_hash().unwrap()
    );
}
