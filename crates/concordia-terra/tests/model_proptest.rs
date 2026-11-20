//! Stateful model test: the world behaves like a map of optional components (M0 phase 0.3).

mod common;

use std::collections::BTreeMap;

use common::{A, B, C, ids};
use concordia_core::id::{DeterministicIdGen, EntityId, IdSource};
use concordia_terra::{CommandBuffer, Pending, Target, World};
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

/// Where a queued command points: a pool entity, or the buffer's latest pending spawn.
#[derive(Debug, Clone, Copy)]
enum Aim {
    Slot(usize),
    Latest,
}

/// A command recorded into a buffer (M2 phase 2.0b), applied when the buffer is.
#[derive(Debug, Clone)]
enum Queued {
    Spawn,
    Despawn(Aim),
    InsertA(Aim, u32),
    InsertB(Aim, f32),
    RemoveA(Aim),
}

#[derive(Debug, Clone)]
enum Step {
    Now(Op),
    Queue(Queued),
    Flush,
}

fn aim() -> impl Strategy<Value = Aim> {
    prop_oneof![(0..POOL).prop_map(Aim::Slot), Just(Aim::Latest)]
}

fn queued() -> impl Strategy<Value = Queued> {
    prop_oneof![
        Just(Queued::Spawn),
        aim().prop_map(Queued::Despawn),
        (aim(), any::<u32>()).prop_map(|(t, v)| Queued::InsertA(t, v)),
        (aim(), -1e6f32..1e6).prop_map(|(t, v)| Queued::InsertB(t, v)),
        aim().prop_map(Queued::RemoveA),
    ]
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        3 => op().prop_map(Step::Now),
        4 => queued().prop_map(Step::Queue),
        1 => Just(Step::Flush),
    ]
}

/// Records `q` into `buffer`; `latest` is the buffer's newest pending spawn.
fn record(buffer: &mut CommandBuffer, latest: &mut Option<Pending>, pool: &[EntityId], q: &Queued) {
    let target = |aim: &Aim| -> Option<Target> {
        match aim {
            Aim::Slot(i) => Some(pool[*i].into()),
            Aim::Latest => latest.map(Into::into),
        }
    };
    match q {
        Queued::Spawn => *latest = Some(buffer.spawn()),
        Queued::Despawn(t) => target(t).into_iter().for_each(|t| buffer.despawn(t)),
        Queued::InsertA(t, v) => target(t).into_iter().for_each(|t| buffer.insert(t, A(*v))),
        Queued::InsertB(t, v) => target(t).into_iter().for_each(|t| buffer.insert(t, B(*v))),
        Queued::RemoveA(t) => target(t).into_iter().for_each(|t| buffer.remove::<A>(t)),
    }
}

/// Applies the queued commands to the model as a buffer would: in order, skipping dead targets.
fn model_flush(
    model: &mut Model,
    ids: &mut DeterministicIdGen,
    pool: &[EntityId],
    queue: &[Queued],
) {
    let mut latest: Option<EntityId> = None;
    for q in queue {
        let resolve = |aim: &Aim| match aim {
            Aim::Slot(i) => Some(pool[*i]),
            Aim::Latest => latest,
        };
        match q {
            Queued::Spawn => {
                let id = ids.next_id().unwrap();
                model.insert(id, (None, None, None));
                latest = Some(id);
            }
            Queued::Despawn(t) => {
                if let Some(id) = resolve(t) {
                    model.remove(&id);
                }
            }
            Queued::InsertA(t, v) => {
                if let Some(e) = resolve(t).and_then(|id| model.get_mut(&id)) {
                    e.0 = Some(A(*v));
                }
            }
            Queued::InsertB(t, v) => {
                if let Some(e) = resolve(t).and_then(|id| model.get_mut(&id)) {
                    e.1 = Some(B(*v));
                }
            }
            Queued::RemoveA(t) => {
                if let Some(e) = resolve(t).and_then(|id| model.get_mut(&id)) {
                    e.0 = None;
                }
            }
        }
    }
}

proptest! {
    #[test]
    fn model_proptest_with_command_buffers_matches_the_model(
        steps in prop::collection::vec(step(), 1..200)
    ) {
        let pool = ids(POOL);
        let mut world = World::new();
        let mut model = Model::new();
        let mut world_ids = DeterministicIdGen::new(99);
        let mut model_ids = world_ids.clone();
        let mut buffer = CommandBuffer::new();
        let mut latest = None;
        let mut queue = Vec::new();
        for step in steps.into_iter().chain([Step::Flush]) {
            match step {
                Step::Now(op) => apply(&mut world, &mut model, &pool, op),
                Step::Queue(q) => {
                    record(&mut buffer, &mut latest, &pool, &q);
                    queue.push(q);
                }
                Step::Flush => {
                    std::mem::take(&mut buffer).apply(&mut world, &mut world_ids);
                    model_flush(&mut model, &mut model_ids, &pool, &queue);
                    queue.clear();
                    latest = None;
                }
            }
            assert_matches(&world, &model);
        }
    }
}
