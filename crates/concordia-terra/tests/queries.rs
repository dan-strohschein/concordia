//! Typed and dynamic queries (M0 phase 0.3).

mod common;

use std::num::NonZeroUsize;

use common::{A, B, C, Pos, Vel, ids, moving_world, registry};
use concordia_core::jobs::JobSystem;
use concordia_terra::{DynamicQuery, EcsError, QueryError, With, Without, World};
use serde_json::json;

fn abc_world() -> (World, Vec<concordia_core::id::EntityId>) {
    let mut world = World::new();
    let ids = ids(4);
    world
        .spawn_with_id(ids[0])
        .unwrap()
        .insert(A(0))
        .unwrap()
        .insert(B(0.0))
        .unwrap();
    world
        .spawn_with_id(ids[1])
        .unwrap()
        .insert(A(1))
        .unwrap()
        .insert(B(1.0))
        .unwrap()
        .insert(C("c".into()))
        .unwrap();
    world.spawn_with_id(ids[2]).unwrap().insert(A(2)).unwrap();
    world
        .spawn_with_id(ids[3])
        .unwrap()
        .insert(B(3.0))
        .unwrap()
        .insert(A(3))
        .unwrap();
    (world, ids)
}

#[test]
fn query_yields_only_matching_archetypes() {
    let (mut world, ids) = abc_world();
    let mut query = world.query_filtered::<(&A, &mut B), Without<C>>().unwrap();
    let mut seen = Vec::new();
    query.for_each(&mut world, |id, (a, b)| seen.push((id, a.0, b.0)));
    seen.sort_by_key(|s| s.0);
    assert_eq!(seen, vec![(ids[0], 0, 0.0), (ids[3], 3, 3.0)]);

    let mut with_c = world.query_filtered::<&A, With<C>>().unwrap();
    let mut count = 0;
    with_c.for_each(&mut world, |_, _| count += 1);
    assert_eq!(count, 1);
}

#[test]
fn query_mut_writes_are_visible() {
    let mut world = moving_world(10);
    let mut query = world.query::<(&mut Pos, &Vel)>().unwrap();
    query.for_each(&mut world, |_, (p, v)| p.x += v.x);
    let first = ids(1)[0];
    assert_eq!(world.get::<Pos>(first).unwrap().x, 1.0);
}

#[test]
fn query_state_picks_up_archetypes_created_later() {
    let (mut world, _) = abc_world();
    let mut query = world.query::<&A>().unwrap();
    let mut count = 0;
    query.for_each(&mut world, |_, _| count += 1);
    assert_eq!(count, 4);
    let id = ids(5)[4];
    world
        .spawn_with_id(id)
        .unwrap()
        .insert(A(9))
        .unwrap()
        .insert(Vel::default())
        .unwrap();
    let mut count = 0;
    query.for_each(&mut world, |_, _| count += 1);
    assert_eq!(count, 5);
}

#[test]
fn aliasing_mutable_access_is_rejected() {
    let world = World::new();
    assert!(matches!(
        world.query::<(&mut A, &A)>(),
        Err(QueryError::Conflict { component: "A" })
    ));
    assert!(
        world.query::<(&A, &A)>().is_err(),
        "each component appears at most once"
    );
    assert!(world.query::<(&mut A, &B)>().is_ok());
}

#[test]
fn query_on_unknown_component_matches_nothing() {
    let (mut world, _) = abc_world();
    let mut query = world.query::<&Pos>().unwrap();
    let mut count = 0;
    query.for_each(&mut world, |_, _| count += 1);
    assert_eq!(count, 0);
}

#[test]
fn par_for_each_mut_matches_sequential_result() {
    let chunk = NonZeroUsize::new(4096).unwrap();
    let run = |threads| {
        let mut world = moving_world(100_000);
        let jobs = JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap();
        let mut query = world.query::<(&mut Pos, &Vel)>().unwrap();
        for _ in 0..3 {
            query.par_for_each(&mut world, &jobs, chunk, |_, (p, v)| {
                p.x += v.x * 0.016;
                p.y += v.y * 0.016;
            });
        }
        world.state_hash().unwrap()
    };
    assert_eq!(run(1), run(8));
}

#[test]
fn dynamic_query_by_names_returns_sorted_entity_ids() {
    let (world, ids) = abc_world();
    let reg = registry();
    let query = DynamicQuery {
        with: vec!["A".into()],
        without: vec!["C".into()],
    };
    assert_eq!(
        query.run(&world, &reg).unwrap(),
        vec![ids[0], ids[2], ids[3]]
    );
    let unknown = DynamicQuery {
        with: vec!["Nope".into()],
        without: vec![],
    };
    assert!(matches!(
        unknown.run(&world, &reg),
        Err(EcsError::ComponentNotRegistered { name }) if name == "Nope"
    ));
}

#[test]
fn dynamic_get_set_json_roundtrip() {
    let (mut world, ids) = abc_world();
    let reg = registry();
    let id = ids[2];
    assert_eq!(world.set_json(id, "B", json!(4.5), &reg).unwrap(), None);
    assert_eq!(world.get_json(id, "B").unwrap(), json!(4.5));
    assert_eq!(
        world.set_json(id, "B", json!(5.0), &reg).unwrap(),
        Some(json!(4.5))
    );
    assert!(world.set_json(id, "B", json!("x"), &reg).is_err());
    let pos = json!({"x": 1.0, "y": 2.0, "z": 3.0});
    world.set_json(id, "Pos", pos.clone(), &reg).unwrap();
    assert_eq!(
        world.get::<Pos>(id),
        Some(&Pos {
            x: 1.0,
            y: 2.0,
            z: 3.0
        })
    );
    assert_eq!(world.get_json(id, "Pos").unwrap(), pos);
    let removed = world.remove_dyn(id, "Pos").unwrap().unwrap();
    assert_eq!(removed.to_json().unwrap(), pos);
    assert!(matches!(
        world.get_json(id, "Pos"),
        Err(EcsError::ComponentMissing { .. })
    ));
}

#[test]
fn component_from_json_validates_and_requires_a_component() {
    let mut reg = registry();
    let value = concordia_terra::component_from_json(&reg, "B", json!(1.5)).unwrap();
    assert_eq!(value.to_json().unwrap(), json!(1.5));
    assert!(matches!(
        concordia_terra::component_from_json(&reg, "B", json!("x")),
        Err(EcsError::Reflect(_))
    ));
    reg.register::<u8>().unwrap();
    assert!(matches!(
        concordia_terra::component_from_json(&reg, "u8", json!(1)),
        Err(EcsError::NotAComponent { .. })
    ));
}
