//! Storage kinds (M2 phase 2.0b, ADR-0069): shared components, chunk components and tags.
//! Mass-style concepts (shared fragments, chunk fragments, tags) from public documentation only.

mod common;

use common::{Marker, Pos, ids};
use concordia_reflect::Reflect;
use concordia_terra::{
    CHUNK_ROWS, EcsError, QueryError, StorageKind, With, Without, World, WorldHash,
};
use serde::{Deserialize, Serialize};

/// A shared component: many entities, few distinct values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Team(pub u32);

/// A chunk component: one value per chunk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Bucket(pub u32);

fn spawn_with(
    world: &mut World,
    n: usize,
    team: impl Fn(usize) -> u32,
) -> Vec<concordia_core::id::EntityId> {
    let all = ids(n);
    for (i, &id) in all.iter().enumerate() {
        world.spawn_with_id(id).unwrap();
        world
            .insert(
                id,
                Pos {
                    x: i as f32,
                    y: 0.0,
                    z: 0.0,
                },
            )
            .unwrap();
        world.insert(id, Team(team(i))).unwrap();
    }
    all
}

#[test]
fn shared_component_is_stored_once_per_archetype_value() {
    let mut world = World::new();
    world.register_shared::<Team>().unwrap();
    assert_eq!(world.storage_kind::<Team>(), Some(StorageKind::Shared));
    let all = spawn_with(&mut world, 300, |i| (i % 3) as u32);
    assert_eq!(
        world.storage_stats().shared_values,
        3,
        "one stored value per distinct Team"
    );
    for (i, id) in all.iter().enumerate() {
        assert_eq!(world.get::<Team>(*id), Some(&Team((i % 3) as u32)));
    }
    let mut seen = vec![0usize; 3];
    world
        .query::<(&Pos, &Team)>()
        .unwrap()
        .for_each(&mut world, |_, (p, t)| {
            assert_eq!(p.x as usize % 3, t.0 as usize);
            seen[t.0 as usize] += 1;
        });
    assert_eq!(seen, [100, 100, 100]);
    // Changing one entity's value moves it; no new stored value for an existing one.
    let t0 = world.change_tick();
    assert_eq!(world.insert(all[0], Team(2)).unwrap(), Some(Team(0)));
    assert_eq!(world.get::<Team>(all[0]), Some(&Team(2)));
    assert_eq!(world.storage_stats().shared_values, 3);
    assert_eq!(world.changes_since(t0).unwrap().changed, vec![all[0]]);
    assert_eq!(world.remove::<Team>(all[1]).unwrap(), Some(Team(1)));
    assert_eq!(world.get::<Team>(all[1]), None);
    world.validate_storage().unwrap();
}

#[test]
fn shared_components_are_read_only_in_queries_and_registered_before_use() {
    let mut world = World::new();
    world.register_shared::<Team>().unwrap();
    spawn_with(&mut world, 4, |_| 1);
    assert!(matches!(
        world.query::<&mut Team>(),
        Err(QueryError::SharedReadOnly { component: "Team" })
    ));
    assert_eq!(
        world.get_mut::<Team>(ids(1)[0]),
        None,
        "change a shared value with insert"
    );
    let mut late = World::new();
    spawn_with(&mut late, 1, |_| 0);
    assert!(matches!(
        late.register_shared::<Team>(),
        Err(EcsError::StorageKindInUse { component: "Team" })
    ));
}

#[test]
fn tags_filter_queries_without_storage() {
    let mut world = World::new();
    let all = ids(100);
    for (i, &id) in all.iter().enumerate() {
        world.spawn_with_id(id).unwrap();
        world.insert(id, Pos::default()).unwrap();
        if i % 4 == 0 {
            world.insert(id, Marker).unwrap();
        }
    }
    assert_eq!(world.storage_kind::<Marker>(), Some(StorageKind::Tag));
    // Two archetypes ({Pos}, {Pos, Marker}) hold one Pos column each; the tag holds none.
    assert_eq!(world.storage_stats().columns, 2);
    let mut tagged = 0;
    world
        .query_filtered::<&Pos, With<Marker>>()
        .unwrap()
        .for_each(&mut world, |_, _| tagged += 1);
    let mut untagged = 0;
    world
        .query_filtered::<&Pos, Without<Marker>>()
        .unwrap()
        .for_each(&mut world, |_, _| untagged += 1);
    assert_eq!((tagged, untagged), (25, 75));
    let mut fetched = 0;
    world
        .query::<(&Pos, &Marker)>()
        .unwrap()
        .for_each(&mut world, |_, _| fetched += 1);
    assert_eq!(fetched, 25);
    assert_eq!(world.get::<Marker>(all[0]), Some(&Marker));
    assert_eq!(world.remove::<Marker>(all[0]).unwrap(), Some(Marker));
    assert_eq!(world.get::<Marker>(all[0]), None);
    world.validate_storage().unwrap();
}

#[test]
fn chunk_component_is_per_chunk_and_survives_moves() {
    let mut world = World::new();
    let n = 2 * CHUNK_ROWS + 10;
    let all = spawn_with(&mut world, n, |_| 0);
    let (first, second, third) = (all[0], all[CHUNK_ROWS], all[2 * CHUNK_ROWS]);
    assert_eq!(world.set_chunk(first, Bucket(1)).unwrap(), None);
    assert_eq!(world.set_chunk(second, Bucket(2)).unwrap(), None);
    // Every row of a chunk sees its value; other chunks don't.
    assert_eq!(world.chunk::<Bucket>(all[CHUNK_ROWS - 1]), Some(&Bucket(1)));
    assert_eq!(world.chunk::<Bucket>(all[CHUNK_ROWS + 1]), Some(&Bucket(2)));
    assert_eq!(world.chunk::<Bucket>(third), None);
    // Despawning in chunk 0 swaps the last row in: the value stays with the chunk, not the entity.
    let last = all[n - 1];
    world.despawn(first).unwrap();
    assert_eq!(
        world.chunk::<Bucket>(last),
        Some(&Bucket(1)),
        "moved into chunk 0"
    );
    // Leaving the archetype leaves the chunk value behind.
    world.insert(second, Marker).unwrap();
    assert_eq!(world.chunk::<Bucket>(second), None);
    assert_eq!(world.chunk::<Bucket>(all[CHUNK_ROWS + 1]), Some(&Bucket(2)));
    // A chunk that empties drops its value; a refilled chunk starts without one.
    world.set_chunk(third, Bucket(3)).unwrap();
    for &id in &all[2 * CHUNK_ROWS - 20..n - 1] {
        let _ = world.despawn(id);
    }
    let again = ids(n + 40)[n..].to_vec();
    for &id in &again {
        world.spawn_with_id(id).unwrap();
        world.insert(id, Pos::default()).unwrap();
        world.insert(id, Team(0)).unwrap();
    }
    assert!(
        again
            .iter()
            .all(|&id| world.chunk::<Bucket>(id) != Some(&Bucket(3)))
    );
    world.validate_storage().unwrap();
}

#[test]
fn chunk_components_are_not_hashed() {
    let mut world = World::new();
    let all = spawn_with(&mut world, 10, |_| 0);
    let before = world.state_hash().unwrap();
    world.set_chunk(all[0], Bucket(7)).unwrap();
    assert_eq!(world.chunk::<Bucket>(all[0]), Some(&Bucket(7)));
    assert_eq!(world.state_hash().unwrap(), before);
}

/// The hash of this world with every component stored densely, computed before 2.0b.
const DENSE_HASH: &str = "63957e40406c75ec69cdf3774ac2d7320838bc61ff6bcea3f45b9a795e683080";

#[test]
fn storage_kinds_never_change_the_world_hash() {
    let mut world = World::new();
    world.register_shared::<Team>().unwrap();
    for (i, id) in ids(12).into_iter().enumerate() {
        world.spawn_with_id(id).unwrap();
        world
            .insert(
                id,
                Pos {
                    x: i as f32,
                    y: 0.0,
                    z: 0.0,
                },
            )
            .unwrap();
        world.insert(id, Team(i as u32 % 3)).unwrap();
        if i % 2 == 0 {
            world.insert(id, Marker).unwrap();
        }
    }
    world.set_chunk(ids(1)[0], Bucket(9)).unwrap();
    assert_eq!(world.storage_kind::<Marker>(), Some(StorageKind::Tag));
    let expected: WorldHash = DENSE_HASH.parse().unwrap();
    assert_eq!(world.state_hash().unwrap(), expected);
}

#[test]
fn shared_and_tag_components_read_by_name() {
    let mut world = World::new();
    world.register_shared::<Team>().unwrap();
    let id = spawn_with(&mut world, 1, |_| 5)[0];
    world.insert(id, Marker).unwrap();
    assert_eq!(world.storage_kind::<Team>(), Some(StorageKind::Shared));
    assert_eq!(world.storage_kind::<Marker>(), Some(StorageKind::Tag));
    assert_eq!(
        world.component_names(id).unwrap(),
        ["Marker", "Pos", "Team"]
    );
    let mut reg = common::registry();
    concordia_terra::register_component::<Team>(&mut reg).unwrap();
    concordia_terra::register_component::<Marker>(&mut reg).unwrap();
    assert_eq!(world.get_json(id, "Team").unwrap(), serde_json::json!(5));
    world
        .set_json(id, "Team", serde_json::json!(6), &reg)
        .unwrap();
    assert_eq!(world.get::<Team>(id), Some(&Team(6)));
    world.remove_dyn(id, "Marker").unwrap();
    assert_eq!(world.get::<Marker>(id), None);
}
