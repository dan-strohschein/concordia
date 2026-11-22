//! `BoneAttachment` (2.4.4): an attachment entity's `Transform` becomes its bone's transform in the
//! posed character's space, so its children follow the bone in the world. Hand-computed.

mod common;

use std::f32::consts::FRAC_PI_2;
use std::num::NonZeroUsize;
use std::sync::Arc;

use common::{close, joint};
use concordia_anim::{AnimLibrary, BoneAttachment, Pose, Skeleton, add_processors};
use concordia_assets::{AssetRef, Skinned};
use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::jobs::JobSystem;
use concordia_core::math::{Mat4, Quat, Vec3};
use concordia_core::time::TickRate;
use concordia_scene::builtin::{Parent, Transform, world_transform};
use concordia_terra::{Schedule, SimLoop, World};

const EPS: f32 = 1e-5;

fn skin() -> AssetRef {
    "asset:018bcfe5-6800-7000-a424-6c55076b137f#skin/0"
        .parse()
        .unwrap()
}

/// `hip` at the origin and `arm` 1 m above it; the bind mesh sits 2 m along +Z of the root.
fn skeleton() -> Skeleton {
    Skeleton::new(vec![
        joint("hip", None, Vec3::ZERO),
        joint("arm", Some(0), Vec3::Y),
    ])
    .unwrap()
    .with_mesh(Mat4::from_translation(Vec3::new(0.0, 0.0, 2.0)))
    .unwrap()
}

fn at(translation: Vec3) -> Transform {
    Transform {
        translation,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    }
}

struct Scene {
    world: World,
    character: EntityId,
    sword: EntityId,
    blade: EntityId,
    lost: EntityId,
    orphan: EntityId,
}

/// A character at x = 10 with its hip turned 90° about Z; a sword attached to its arm with a
/// blade child 0.5 m up the sword; an attachment naming a bone that doesn't exist; and one
/// whose parent has no pose.
fn scene() -> Scene {
    let mut world = World::new();
    let mut ids = DeterministicIdGen::new(5);
    let character = world.spawn(&mut ids).unwrap();
    world
        .insert(character, at(Vec3::new(10.0, 0.0, 0.0)))
        .unwrap();
    world.insert(character, Skinned { skin: skin() }).unwrap();
    let mut pose = skeleton().rest_pose();
    pose.bones[0].rotation = Quat::from_rotation_z(FRAC_PI_2);
    world.insert(character, pose).unwrap();
    let child = |world: &mut World, ids: &mut DeterministicIdGen, parent, bone: Option<&str>, t| {
        let id = world.spawn(ids).unwrap();
        world.insert(id, Parent(parent)).unwrap();
        world.insert(id, at(t)).unwrap();
        if let Some(bone) = bone {
            world
                .insert(
                    id,
                    BoneAttachment {
                        bone: bone.to_owned(),
                    },
                )
                .unwrap();
        }
        id
    };
    let sword = child(&mut world, &mut ids, character, Some("arm"), Vec3::ZERO);
    let blade = child(&mut world, &mut ids, sword, None, Vec3::new(0.0, 0.5, 0.0));
    let lost = child(
        &mut world,
        &mut ids,
        character,
        Some("tail"),
        Vec3::splat(7.0),
    );
    let prop = world.spawn(&mut ids).unwrap();
    world.insert(prop, at(Vec3::ZERO)).unwrap();
    let orphan = child(&mut world, &mut ids, prop, Some("arm"), Vec3::splat(8.0));
    Scene {
        world,
        character,
        sword,
        blade,
        lost,
        orphan,
    }
}

fn run(scene: &mut Scene, ticks: u32) {
    let mut library = AnimLibrary::new();
    library.insert_skeleton(skin(), skeleton());
    let mut schedule = Schedule::new();
    add_processors(&mut schedule, Arc::new(library));
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut scene.world).unwrap();
    let jobs = JobSystem::new(NonZeroUsize::new(2).unwrap()).unwrap();
    sim.step(&mut scene.world, &jobs, ticks);
}

#[test]
fn bone_attachment_follows_bone_world_transform() {
    let mut scene = scene();
    run(&mut scene, 1);
    // The arm in model space: the hip's 90° turn takes (0, 1, 0) to (-1, 0, 0). In the
    // character's space the bind mesh's +2 Z is taken off: (-1, 0, -2), turned 90° about Z.
    let sword = *scene.world.get::<Transform>(scene.sword).unwrap();
    assert!(
        close(sword.translation, Vec3::new(-1.0, 0.0, -2.0), EPS),
        "{sword:?}"
    );
    assert!(
        sword
            .rotation
            .angle_between(Quat::from_rotation_z(FRAC_PI_2))
            < EPS,
        "{sword:?}"
    );
    assert!(close(sword.scale, Vec3::ONE, EPS));
    // The blade, 0.5 m up the sword: turned to (-0.5, 0, 0), so (8.5, 0, -2) in the world.
    let blade = world_transform(&scene.world, scene.blade).unwrap();
    let tip = blade.transform_point3(Vec3::ZERO);
    assert!(close(tip, Vec3::new(8.5, 0.0, -2.0), EPS), "{tip}");
    // It agrees with the skeleton's own bone in the character's space times the world matrix.
    let character = world_transform(&scene.world, scene.character).unwrap();
    let pose = scene.world.get::<Pose>(scene.character).unwrap().clone();
    let bone = skeleton().entity_space(&pose).unwrap()[1];
    let expected = (character * bone).transform_point3(Vec3::new(0.0, 0.5, 0.0));
    assert!(close(tip, expected, EPS), "{tip} vs {expected}");

    // A missing bone and a parent without a pose leave the Transform alone.
    assert_eq!(
        scene
            .world
            .get::<Transform>(scene.lost)
            .unwrap()
            .translation,
        Vec3::splat(7.0)
    );
    assert_eq!(
        scene
            .world
            .get::<Transform>(scene.orphan)
            .unwrap()
            .translation,
        Vec3::splat(8.0)
    );

    // A new pose moves it on the next tick: hip back to rest puts the arm at (0, 1, -2).
    let rest = skeleton().rest_pose();
    scene.world.insert(scene.character, rest).unwrap();
    run(&mut scene, 1);
    let sword = *scene.world.get::<Transform>(scene.sword).unwrap();
    assert!(
        close(sword.translation, Vec3::new(0.0, 1.0, -2.0), EPS),
        "{sword:?}"
    );
}
