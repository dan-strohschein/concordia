//! Determinism: repeatable, thread-count independent, order independent, save/restore (M1 phase 1.7b).

mod common;

use common::Sim;
use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_physics::{Collider, ColliderShape, PhysicsSettings, RigidBody};
use concordia_scene::builtin::Transform;

/// 100 bodies in a deterministic pseudo-random layout, inserted in `order`.
fn pile(threads: usize, order: &[usize]) -> Sim {
    let mut sim = Sim::with(PhysicsSettings::default(), threads);
    sim.ground();
    let ids: Vec<EntityId> = (0..100)
        .map(|i| {
            format!("0193a1c2-7e40-7000-8000-{:012x}", 100 + i)
                .parse()
                .unwrap()
        })
        .collect();
    for &i in order {
        let at = Vec3::new(
            (i % 10) as f32 * 0.9 - 4.0,
            1.0 + (i / 10) as f32 * 1.1,
            ((i * 7) % 5) as f32 * 0.3,
        );
        let shape = match i % 3 {
            0 => ColliderShape::Sphere { radius: 0.4 },
            1 => ColliderShape::Box {
                half_extents: Vec3::new(0.4, 0.3, 0.35),
            },
            _ => ColliderShape::Capsule {
                half_height: 0.3,
                radius: 0.25,
            },
        };
        sim.world.spawn_with_id(ids[i]).unwrap();
        sim.insert(ids[i], at, RigidBody::dynamic(), Collider::new(shape));
    }
    sim
}

fn in_order() -> Vec<usize> {
    (0..100).collect()
}

#[test]
fn same_simulation_twice_produces_identical_state_hash() {
    let mut a = pile(1, &in_order());
    let mut b = pile(1, &in_order());
    a.step(600);
    b.step(600);
    assert_eq!(a.physics.state_hash(), b.physics.state_hash());
}

#[test]
fn state_hash_identical_across_thread_counts_1_2_8() {
    let hashes: Vec<_> = [1, 2, 8]
        .into_iter()
        .map(|threads| {
            let mut sim = pile(threads, &in_order());
            sim.step(300);
            sim.physics.state_hash()
        })
        .collect();
    assert_eq!(hashes[0], hashes[1]);
    assert_eq!(hashes[0], hashes[2]);
}

#[test]
fn spawn_order_in_scene_does_not_change_outcome() {
    let mut shuffled: Vec<usize> = in_order();
    shuffled.sort_by_key(|i| (i * 37) % 101);
    let mut a = pile(1, &in_order());
    let mut b = pile(1, &shuffled);
    a.step(300);
    b.step(300);
    assert_eq!(a.physics.state_hash(), b.physics.state_hash());
}

#[test]
fn save_restore_then_step_equals_uninterrupted_run() {
    let mut sim = pile(2, &in_order());
    sim.step(300);
    let saved = sim.physics.save_state();
    let world_at_300 = sim.world.clone();
    sim.step(300);
    let uninterrupted = sim.physics.state_hash();
    sim.physics.restore_state(&saved).unwrap();
    sim.world = world_at_300;
    sim.step(300);
    assert_eq!(sim.physics.state_hash(), uninterrupted);
}

#[test]
fn box_stack_hash_matches_committed_snapshot() {
    let mut sim = Sim::new();
    sim.ground();
    for (row, count) in [4, 3, 2, 1].into_iter().enumerate() {
        for i in 0..count {
            let x = i as f32 * 1.05 - (count - 1) as f32 * 0.525;
            sim.cube(Vec3::new(x, 0.5 + row as f32 * 1.0, 0.0));
        }
    }
    let dropped = sim.cube(Vec3::new(0.3, 6.0, 0.2));
    sim.world.get_mut::<Transform>(dropped).unwrap().rotation =
        concordia_core::math::Quat::from_rotation_y(0.4)
            * concordia_core::math::Quat::from_rotation_x(0.2);
    sim.step(180);
    insta::assert_snapshot!(sim.physics.state_hash().to_string());
}
