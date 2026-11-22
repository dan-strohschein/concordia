//! Built-in components and the default headless system (M0 phase 0.5).

use std::num::NonZeroUsize;

use concordia_core::id::DeterministicIdGen;
use concordia_core::jobs::JobSystem;
use concordia_core::math::{Quat, Vec3};
use concordia_core::time::TickRate;
use concordia_reflect::TypeRegistry;
use concordia_scene::builtin::{self, Transform, Velocity};
use concordia_terra::{SimLoop, World};

#[test]
fn builtins_register_with_docs_and_schemas() {
    let mut reg = TypeRegistry::new();
    builtin::register(&mut reg).unwrap();
    for name in ["Name", "Transform", "Parent", "Velocity"] {
        let schema = reg.schema_for(name).unwrap();
        assert!(
            !schema["description"].as_str().unwrap_or("").is_empty(),
            "{name} has docs"
        );
        assert!(
            reg.get_by_name(name)
                .unwrap()
                .data::<concordia_terra::ReflectComponent>()
                .is_some()
        );
    }
}

#[test]
fn transform_default_is_identity() {
    assert_eq!(
        Transform::default(),
        Transform {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE
        }
    );
}

#[test]
fn integrate_velocity_moves_and_rotates_each_tick() {
    let mut world = World::new();
    let id = world.spawn(&mut DeterministicIdGen::new(1)).unwrap();
    world.insert(id, Transform::default()).unwrap();
    world
        .insert(
            id,
            Velocity {
                linear: Vec3::new(60.0, 0.0, 0.0),
                angular: Vec3::new(0.0, 60.0, 0.0),
            },
        )
        .unwrap();
    let still = world.spawn(&mut DeterministicIdGen::new(2)).unwrap();
    world.insert(still, Transform::default()).unwrap();

    let jobs = JobSystem::new(NonZeroUsize::new(2).unwrap()).unwrap();
    SimLoop::new(TickRate::HZ_60, builtin::headless_schedule(), &mut world)
        .unwrap()
        .step(&mut world, &jobs, 1);
    let t = world.get::<Transform>(id).unwrap();
    assert!((t.translation.x - 1.0).abs() < 1e-5);
    assert!((t.rotation.angle_between(Quat::IDENTITY) - 1.0).abs() < 1e-4);
    assert_eq!(world.get::<Transform>(still), Some(&Transform::default()));
}

fn placed(world: &mut World, seed: u64, x: f32) -> concordia_core::id::EntityId {
    let id = world.spawn(&mut DeterministicIdGen::new(seed)).unwrap();
    let mut t = Transform::default();
    t.translation.x = x;
    world.insert(id, t).unwrap();
    id
}

#[test]
fn lod_position_is_the_world_translation() {
    let mut world = World::new();
    let root = placed(&mut world, 1, 100.0);
    let child = placed(&mut world, 2, 5.0);
    world.insert(child, builtin::Parent(root)).unwrap();
    world.get_mut::<Transform>(root).unwrap().scale = Vec3::splat(2.0);
    assert_eq!(builtin::lod_position(&world, root), Some([100.0, 0.0, 0.0]));
    assert_eq!(
        builtin::lod_position(&world, child),
        Some([110.0, 0.0, 0.0])
    );
    let bare = world.spawn(&mut DeterministicIdGen::new(3)).unwrap();
    assert_eq!(builtin::lod_position(&world, bare), None);
}

#[test]
fn headless_schedule_runs_simulation_lod_from_world_positions() {
    use concordia_terra::{LodProfile, LodViewer, SimLod};
    let names: Vec<String> = builtin::headless_schedule()
        .names()
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(names, ["terra.lod", "integrate_velocity"]);

    let mut world = World::new();
    world.register_shared::<LodProfile>().unwrap();
    let rig = placed(&mut world, 1, 100.0);
    let viewer = placed(&mut world, 2, 0.0);
    world.insert(viewer, builtin::Parent(rig)).unwrap();
    world.insert(viewer, LodViewer).unwrap();
    let far = placed(&mut world, 3, 130.0);
    world
        .insert(
            far,
            LodProfile::new([10.0, 20.0, 40.0], [2, 4, 8], 1.0).unwrap(),
        )
        .unwrap();
    let jobs = JobSystem::new(NonZeroUsize::new(2).unwrap()).unwrap();
    let mut sim = SimLoop::new(TickRate::HZ_60, builtin::headless_schedule(), &mut world).unwrap();
    sim.step(&mut world, &jobs, 1);
    // 30 m from the viewer's world position (the rig's 100 m), not its local 0.
    assert_eq!(world.get::<SimLod>(far).unwrap().level, 2);
}
