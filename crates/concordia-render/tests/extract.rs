//! Extracting a render snapshot from the ECS (M1 phase 1.5).

use concordia_assets::{AssetRef, MeshRenderer};
use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::math::{Quat, Vec3};
use concordia_render::{Camera, DirectionalLight, RenderError, RenderWorld};
use concordia_scene::builtin::{Parent, Transform};
use concordia_terra::World;

fn at(world: &mut World, ids: &mut DeterministicIdGen, translation: Vec3) -> EntityId {
    let id = world.spawn(ids).unwrap();
    world
        .insert(
            id,
            Transform {
                translation,
                ..Transform::default()
            },
        )
        .unwrap();
    id
}

fn cube() -> MeshRenderer {
    MeshRenderer {
        mesh: "builtin:mesh/cube".parse::<AssetRef>().unwrap(),
        material: "builtin:material/default".parse().unwrap(),
        cast_shadows: true,
    }
}

#[test]
fn extract_collects_draws_lights_and_cameras_with_world_transforms() {
    let (mut world, mut ids) = (World::new(), DeterministicIdGen::new(1));
    let parent = at(&mut world, &mut ids, Vec3::new(5.0, 0.0, 0.0));
    let child = at(&mut world, &mut ids, Vec3::new(1.0, 0.0, 0.0));
    world.insert(child, Parent(parent)).unwrap();
    world.insert(child, cube()).unwrap();
    let sun = at(&mut world, &mut ids, Vec3::ZERO);
    world.get_mut::<Transform>(sun).unwrap().rotation =
        Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    world.insert(sun, DirectionalLight::default()).unwrap();
    assert!(
        DirectionalLight::default().shadows.is_none(),
        "shadows are opt-in"
    );
    let cam = at(&mut world, &mut ids, Vec3::new(0.0, 0.0, 10.0));
    world.insert(cam, Camera::default()).unwrap();

    let rw = RenderWorld::extract(&world);
    assert_eq!(rw.draws.len(), 1);
    assert!(
        rw.draws[0]
            .world
            .transform_point3(Vec3::ZERO)
            .abs_diff_eq(Vec3::new(6.0, 0.0, 0.0), 1e-5)
    );
    assert_eq!(rw.lights.len(), 1);
    assert!(
        rw.lights[0].direction.abs_diff_eq(Vec3::NEG_Y, 1e-5),
        "light shines along its -Z: {}",
        rw.lights[0].direction
    );
    assert_eq!(rw.cameras.len(), 1);
}

#[test]
fn default_camera_is_highest_priority_then_lowest_id() {
    let (mut world, mut ids) = (World::new(), DeterministicIdGen::new(2));
    let low = at(&mut world, &mut ids, Vec3::ZERO);
    world
        .insert(
            low,
            Camera {
                priority: 0,
                ..Camera::default()
            },
        )
        .unwrap();
    let high = at(&mut world, &mut ids, Vec3::ZERO);
    world
        .insert(
            high,
            Camera {
                priority: 5,
                ..Camera::default()
            },
        )
        .unwrap();
    let rw = RenderWorld::extract(&world);
    assert_eq!(rw.camera(None).unwrap().entity, high);
    assert_eq!(rw.camera(Some(low)).unwrap().entity, low);
}

#[test]
fn unknown_or_missing_cameras_are_descriptive_errors() {
    let (mut world, mut ids) = (World::new(), DeterministicIdGen::new(3));
    let not_a_camera = at(&mut world, &mut ids, Vec3::ZERO);
    let rw = RenderWorld::extract(&world);
    assert!(matches!(rw.camera(None), Err(RenderError::NoCamera)));
    let err = rw.camera(Some(not_a_camera)).unwrap_err();
    assert!(
        matches!(err, RenderError::UnknownCamera(id) if id == not_a_camera),
        "{err}"
    );
}

#[test]
fn extraction_order_is_by_entity_id() {
    let (mut world, mut ids) = (World::new(), DeterministicIdGen::new(4));
    let spawned: Vec<EntityId> = (0..5)
        .map(|i| at(&mut world, &mut ids, Vec3::splat(i as f32)))
        .collect();
    for id in spawned.iter().rev() {
        world.insert(*id, cube()).unwrap();
    }
    let order: Vec<EntityId> = RenderWorld::extract(&world)
        .draws
        .iter()
        .map(|d| d.entity)
        .collect();
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(order, sorted);
}

#[test]
fn render_components_register_with_schemas() {
    let mut registry = concordia_reflect::TypeRegistry::new();
    concordia_render::register_types(&mut registry).unwrap();
    for name in ["Camera", "DirectionalLight"] {
        assert!(registry.schema_for(name).is_some(), "{name}");
    }
}

#[test]
fn point_and_spot_lights_are_extracted_in_world_space() {
    use concordia_render::{PointLight, PunctualKind, SpotLight};
    let (mut world, mut ids) = (World::new(), DeterministicIdGen::new(5));
    let lamp = at(&mut world, &mut ids, Vec3::new(1.0, 2.0, 3.0));
    world
        .insert(
            lamp,
            PointLight {
                range: 7.0,
                ..PointLight::default()
            },
        )
        .unwrap();
    let spot = at(&mut world, &mut ids, Vec3::new(0.0, 5.0, 0.0));
    world.get_mut::<Transform>(spot).unwrap().rotation =
        Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    world
        .insert(
            spot,
            SpotLight {
                inner_angle: 0.2,
                outer_angle: 0.4,
                ..SpotLight::default()
            },
        )
        .unwrap();
    let rw = RenderWorld::extract(&world);
    assert_eq!(rw.punctual.len(), 2);
    assert!(
        rw.punctual[0]
            .position
            .abs_diff_eq(Vec3::new(1.0, 2.0, 3.0), 1e-6)
            && rw.punctual[0].range == 7.0
    );
    assert!(matches!(rw.punctual[0].kind, PunctualKind::Point));
    assert!(
        rw.punctual[1].direction.abs_diff_eq(Vec3::NEG_Y, 1e-5),
        "{}",
        rw.punctual[1].direction
    );
    assert!(
        matches!(rw.punctual[1].kind, PunctualKind::Spot { inner, outer } if inner == 0.2 && outer == 0.4)
    );
}

#[test]
fn light_components_register() {
    let mut registry = concordia_reflect::TypeRegistry::new();
    concordia_render::register_types(&mut registry).unwrap();
    assert!(
        registry.schema_for("PointLight").is_some() && registry.schema_for("SpotLight").is_some()
    );
}
