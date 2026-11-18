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
    SimLoop::new(TickRate::HZ_60, builtin::headless_schedule()).step(&mut world, &jobs, 1);
    let t = world.get::<Transform>(id).unwrap();
    assert!((t.translation.x - 1.0).abs() < 1e-5);
    assert!((t.rotation.angle_between(Quat::IDENTITY) - 1.0).abs() < 1e-4);
    assert_eq!(world.get::<Transform>(still), Some(&Transform::default()));
}
