//! World transforms from the `Parent` hierarchy (M1 phase 1.5).

use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::math::{Quat, Vec3};
use concordia_scene::builtin::{Parent, Transform, world_transforms};
use concordia_terra::World;

fn spawn(
    world: &mut World,
    ids: &mut DeterministicIdGen,
    t: Vec3,
    parent: Option<EntityId>,
) -> EntityId {
    let id = world.spawn(ids).unwrap();
    world
        .insert(
            id,
            Transform {
                translation: t,
                ..Transform::default()
            },
        )
        .unwrap();
    if let Some(p) = parent {
        world.insert(id, Parent(p)).unwrap();
    }
    id
}

#[test]
fn child_world_transform_composes_the_parent_chain() {
    let (mut world, mut ids) = (World::new(), DeterministicIdGen::new(1));
    let root = spawn(&mut world, &mut ids, Vec3::new(10.0, 0.0, 0.0), None);
    world.get_mut::<Transform>(root).unwrap().rotation =
        Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    world.get_mut::<Transform>(root).unwrap().scale = Vec3::splat(2.0);
    let child = spawn(&mut world, &mut ids, Vec3::new(1.0, 0.0, 0.0), Some(root));
    let grandchild = spawn(&mut world, &mut ids, Vec3::new(0.0, 1.0, 0.0), Some(child));
    let result = world_transforms(&world);
    assert!(result.problems.is_empty(), "{:?}", result.problems);
    let at = |id| result.get(id).unwrap().transform_point3(Vec3::ZERO);
    assert!(
        at(child).abs_diff_eq(Vec3::new(10.0, 0.0, -2.0), 1e-5),
        "{}",
        at(child)
    );
    assert!(
        at(grandchild).abs_diff_eq(Vec3::new(10.0, 2.0, -2.0), 1e-5),
        "{}",
        at(grandchild)
    );
}

#[test]
fn missing_parents_and_cycles_are_reported_and_treated_as_roots() {
    let (mut world, mut ids) = (World::new(), DeterministicIdGen::new(2));
    let ghost: EntityId = "0193a1c2-7e40-7000-8000-00000000dead".parse().unwrap();
    let orphan = spawn(&mut world, &mut ids, Vec3::X, Some(ghost));
    let a = spawn(&mut world, &mut ids, Vec3::Y, None);
    let b = spawn(&mut world, &mut ids, Vec3::Z, Some(a));
    world.insert(a, Parent(b)).unwrap();
    let result = world_transforms(&world);
    assert_eq!(result.problems.len(), 2, "{:?}", result.problems);
    assert!(
        result
            .get(orphan)
            .unwrap()
            .transform_point3(Vec3::ZERO)
            .abs_diff_eq(Vec3::X, 1e-6)
    );
    assert!(
        result.get(a).is_some() && result.get(b).is_some(),
        "cycle members still get transforms"
    );
}

#[test]
fn entities_without_transform_have_none() {
    let (mut world, mut ids) = (World::new(), DeterministicIdGen::new(3));
    let bare = world.spawn(&mut ids).unwrap();
    assert!(world_transforms(&world).get(bare).is_none());
}
