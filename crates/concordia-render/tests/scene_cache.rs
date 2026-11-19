//! Incremental extraction (M1 phase 1.9e): the scene cache sends only what changed, and what it
//! assembles always equals a full `RenderWorld::extract`.

use concordia_assets::{AssetRef, MeshRenderer};
use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::math::Vec3;
use concordia_render::{Camera, PointLight, RenderScene, RenderWorld, SceneCache, SceneDelta};
use concordia_scene::builtin::{Parent, Transform};
use concordia_terra::World;
use proptest::prelude::*;

fn cube() -> MeshRenderer {
    MeshRenderer {
        mesh: "builtin:mesh/cube".parse::<AssetRef>().unwrap(),
        material: "builtin:material/default".parse().unwrap(),
        cast_shadows: true,
    }
}

fn spawn(world: &mut World, ids: &mut DeterministicIdGen, x: f32) -> EntityId {
    let id = world.spawn(ids).unwrap();
    let transform = Transform {
        translation: Vec3::new(x, 0.0, 0.0),
        ..Transform::default()
    };
    world.insert(id, transform).unwrap();
    world.insert(id, cube()).unwrap();
    id
}

#[test]
fn an_unchanged_world_sends_an_empty_delta() {
    let (mut world, mut ids) = (World::new(), DeterministicIdGen::new(1));
    for i in 0..5 {
        spawn(&mut world, &mut ids, i as f32);
    }
    let mut cache = SceneCache::default();
    assert!(matches!(cache.update(&world), SceneDelta::Full(_)));
    match cache.update(&world) {
        SceneDelta::Changes { upserts, removed } => {
            assert!(upserts.is_empty() && removed.is_empty())
        }
        SceneDelta::Full(_) => panic!("a known world is updated incrementally"),
    }
}

#[test]
fn moving_a_parent_resends_it_and_its_descendants_only() {
    let (mut world, mut ids) = (World::new(), DeterministicIdGen::new(1));
    let root = spawn(&mut world, &mut ids, 0.0);
    let child = spawn(&mut world, &mut ids, 1.0);
    let other = spawn(&mut world, &mut ids, 5.0);
    world.insert(child, Parent(root)).unwrap();
    let mut cache = SceneCache::default();
    cache.update(&world);
    world.get_mut::<Transform>(root).unwrap().translation.x = 10.0;
    let SceneDelta::Changes { upserts, .. } = cache.update(&world) else {
        panic!("incremental")
    };
    let mut sent: Vec<_> = upserts.iter().map(|(id, _)| *id).collect();
    sent.sort();
    let mut expected = vec![root, child];
    expected.sort();
    assert_eq!(sent, expected, "{other} did not change");
}

#[derive(Debug, Clone)]
enum Op {
    Spawn(f32),
    Move(usize, f32),
    Despawn(usize),
    Reparent(usize, usize),
    Unparent(usize),
    Light(usize),
    Camera(usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (-5.0f32..5.0).prop_map(Op::Spawn),
        (0usize..16, -5.0f32..5.0).prop_map(|(i, x)| Op::Move(i, x)),
        (0usize..16).prop_map(Op::Despawn),
        (0usize..16, 0usize..16).prop_map(|(a, b)| Op::Reparent(a, b)),
        (0usize..16).prop_map(Op::Unparent),
        (0usize..16).prop_map(Op::Light),
        (0usize..16).prop_map(Op::Camera),
    ]
}

fn apply(world: &mut World, ids: &mut DeterministicIdGen, live: &mut Vec<EntityId>, op: &Op) {
    let pick = |i: usize| (!live.is_empty()).then(|| live[i % live.len()]);
    match *op {
        Op::Spawn(x) => live.push(spawn(world, ids, x)),
        Op::Move(i, x) => {
            if let Some(id) = pick(i) {
                world.get_mut::<Transform>(id).unwrap().translation.y = x;
            }
        }
        Op::Despawn(i) => {
            if let Some(id) = pick(i) {
                world.despawn(id).unwrap();
                live.retain(|e| *e != id);
            }
        }
        Op::Reparent(a, b) => {
            // Only parent to an older entity (earlier in `live`), so there are no cycles; cycles are
            // reported errors whose resolution depends on traversal order.
            if !live.is_empty() {
                let (x, y) = (a % live.len(), b % live.len());
                if x != y {
                    world
                        .insert(live[x.max(y)], Parent(live[x.min(y)]))
                        .unwrap();
                }
            }
        }
        Op::Unparent(i) => {
            if let Some(id) = pick(i) {
                world.remove::<Parent>(id).unwrap();
            }
        }
        Op::Light(i) => {
            if let Some(id) = pick(i) {
                world.insert(id, PointLight::default()).unwrap();
            }
        }
        Op::Camera(i) => {
            if let Some(id) = pick(i) {
                world.insert(id, Camera::default()).unwrap();
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn incremental_updates_match_a_full_extract(batches in prop::collection::vec(prop::collection::vec(op(), 0..6), 1..12)) {
        let (mut world, mut ids) = (World::new(), DeterministicIdGen::new(3));
        let mut live = Vec::new();
        let (mut cache, mut mirror) = (SceneCache::default(), RenderScene::default());
        for batch in &batches {
            for op in batch {
                apply(&mut world, &mut ids, &mut live, op);
            }
            mirror.apply(cache.update(&world));
            prop_assert_eq!(mirror.world(), &RenderWorld::extract(&world));
        }
    }
}

#[test]
fn play_scenes_blend_between_the_last_two_ticks() {
    let (mut world, mut ids) = (World::new(), DeterministicIdGen::new(1));
    let moving = spawn(&mut world, &mut ids, 0.0);
    let still = spawn(&mut world, &mut ids, 5.0);
    let (mut cache, mut mirror) = (SceneCache::default(), RenderScene::default());
    mirror.apply_tick(cache.update(&world));
    world.get_mut::<Transform>(moving).unwrap().translation.x = 2.0;
    mirror.apply_tick(cache.update(&world));
    let x_of = |w: &RenderWorld, id| {
        w.draws
            .iter()
            .find(|d| d.entity == id)
            .unwrap()
            .world
            .transform_point3(Vec3::ZERO)
            .x
    };
    let half = mirror.world_at(0.5);
    assert!(
        (x_of(&half, moving) - 1.0).abs() < 1e-5,
        "halfway between ticks"
    );
    assert!(
        (x_of(&half, still) - 5.0).abs() < 1e-5,
        "unmoved entities stay put"
    );
    assert!((x_of(&mirror.world_at(0.0), moving)).abs() < 1e-5);
    assert!((x_of(&mirror.world_at(1.0), moving) - 2.0).abs() < 1e-5);
    assert_eq!(
        mirror.world_at(1.0),
        *mirror.world(),
        "alpha 1 is the latest tick"
    );
}

/// An environment has no position: an entity with only `Environment` still lights the scene, in
/// full extraction and through the scene cache alike.
#[test]
fn an_environment_needs_no_transform() {
    let (mut world, mut ids) = (World::new(), DeterministicIdGen::new(3));
    spawn(&mut world, &mut ids, 0.0);
    let sky = world.spawn(&mut ids).unwrap();
    let environment = concordia_render::Environment {
        map: "asset:01900000-0000-7000-8000-000000000001"
            .parse()
            .unwrap(),
        intensity: 2.0,
        skybox: true,
    };
    world.insert(sky, environment.clone()).unwrap();
    assert_eq!(
        RenderWorld::extract(&world).environment,
        Some(environment.clone())
    );
    let mut cache = SceneCache::default();
    let mut scene = RenderScene::default();
    scene.apply(cache.update(&world));
    assert_eq!(scene.world().environment, Some(environment));
}
