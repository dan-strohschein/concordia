//! Stateful model test: the world behaves like a map of optional components (M0 phase 0.3).

mod common;

use std::collections::BTreeMap;

use common::{A, B, C, ids};
use concordia_core::id::EntityId;
use concordia_terra::World;
use proptest::prelude::*;

type Model = BTreeMap<EntityId, (Option<A>, Option<B>, Option<C>)>;

#[derive(Debug, Clone)]
enum Op {
    Spawn(usize),
    Despawn(usize),
    InsertA(usize, u32),
    InsertB(usize, f32),
    InsertC(usize, String),
    RemoveA(usize),
    RemoveB(usize),
    RemoveC(usize),
}

const POOL: usize = 16;

fn op() -> impl Strategy<Value = Op> {
    let slot = 0..POOL;
    prop_oneof![
        slot.clone().prop_map(Op::Spawn),
        slot.clone().prop_map(Op::Despawn),
        (slot.clone(), any::<u32>()).prop_map(|(i, v)| Op::InsertA(i, v)),
        (slot.clone(), -1e6f32..1e6).prop_map(|(i, v)| Op::InsertB(i, v)),
        (slot.clone(), "[a-z]{0,8}").prop_map(|(i, v)| Op::InsertC(i, v)),
        slot.clone().prop_map(Op::RemoveA),
        slot.clone().prop_map(Op::RemoveB),
        slot.prop_map(Op::RemoveC),
    ]
}

fn apply(world: &mut World, model: &mut Model, pool: &[EntityId], op: Op) {
    match op {
        Op::Spawn(i) => {
            let ok = world.spawn_with_id(pool[i]).is_ok();
            assert_eq!(ok, !model.contains_key(&pool[i]));
            model.entry(pool[i]).or_default();
        }
        Op::Despawn(i) => {
            assert_eq!(
                world.despawn(pool[i]).is_ok(),
                model.remove(&pool[i]).is_some()
            );
        }
        Op::InsertA(i, v) => set(world, model, pool[i], A(v), |m| &mut m.0),
        Op::InsertB(i, v) => set(world, model, pool[i], B(v), |m| &mut m.1),
        Op::InsertC(i, v) => set(world, model, pool[i], C(v), |m| &mut m.2),
        Op::RemoveA(i) => unset::<A>(world, model, pool[i], |m| &mut m.0),
        Op::RemoveB(i) => unset::<B>(world, model, pool[i], |m| &mut m.1),
        Op::RemoveC(i) => unset::<C>(world, model, pool[i], |m| &mut m.2),
    }
}

type Slot<T> = fn(&mut (Option<A>, Option<B>, Option<C>)) -> &mut Option<T>;

fn set<T: concordia_terra::Component + std::fmt::Debug>(
    world: &mut World,
    model: &mut Model,
    id: EntityId,
    value: T,
    slot: Slot<T>,
) {
    let result = world.insert(id, value.clone());
    match model.get_mut(&id) {
        Some(entry) => assert_eq!(result.unwrap(), slot(entry).replace(value)),
        None => assert!(result.is_err()),
    }
}

fn unset<T: concordia_terra::Component + std::fmt::Debug>(
    world: &mut World,
    model: &mut Model,
    id: EntityId,
    slot: Slot<T>,
) {
    let result = world.remove::<T>(id);
    match model.get_mut(&id) {
        Some(entry) => assert_eq!(result.unwrap(), slot(entry).take()),
        None => assert!(result.is_err()),
    }
}

fn assert_matches(world: &World, model: &Model) {
    assert_eq!(world.len(), model.len());
    assert_eq!(
        world.entity_ids_sorted(),
        model.keys().copied().collect::<Vec<_>>()
    );
    for (id, (a, b, c)) in model {
        assert_eq!(world.get::<A>(*id), a.as_ref());
        assert_eq!(world.get::<B>(*id), b.as_ref());
        assert_eq!(world.get::<C>(*id), c.as_ref());
    }
    world.validate_storage().unwrap();
}

proptest! {
    #[test]
    fn world_matches_hashmap_model_under_random_ops(ops in prop::collection::vec(op(), 1..200)) {
        let pool = ids(POOL);
        let mut world = World::new();
        let mut model = Model::new();
        for op in ops {
            apply(&mut world, &mut model, &pool, op);
            assert_matches(&world, &model);
        }
    }
}
