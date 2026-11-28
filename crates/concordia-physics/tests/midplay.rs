//! Bodies changing during play (M8 8.0c): breaking replaces a body by its pieces mid-play. New
//! bodies start with their `Velocity`, removed entities lose theirs, and the result is the same at
//! any thread count and on a rerun. (The mechanism exists since M1/M2; these pin it for M8.)

mod common;

use common::Sim;
use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_physics::{Collider, ColliderShape, PhysicsSettings, RigidBody};
use concordia_scene::builtin::Velocity;

fn cube(centre: Vec3, half: f32) -> Vec<Vec3> {
    (0..8)
        .map(|i| {
            let s = |bit: i32| if i & bit == 0 { -half } else { half };
            centre + Vec3::new(s(1), s(2), s(4))
        })
        .collect()
}

fn id(n: u64) -> EntityId {
    format!("0193a1c2-7e40-7000-8000-{n:012x}").parse().unwrap()
}

/// A two-cube compound thrown sideways; at tick 30 it is replaced by its two cubes, each a hull
/// with the compound's velocity (a split, as 8.3 will do it). Returns the sim after `ticks`.
fn split_run(threads: usize, gravity: bool, ticks: u32) -> (Sim, [EntityId; 2], Velocity) {
    // Undamped, so a tick changes velocity only by gravity (none in the momentum test).
    let body = || RigidBody {
        linear_damping: 0.0,
        angular_damping: 0.0,
        ..RigidBody::dynamic()
    };
    let mut settings = PhysicsSettings::default();
    if !gravity {
        settings.gravity = Vec3::ZERO;
    }
    let mut sim = Sim::with(settings, threads);
    if gravity {
        sim.ground();
    }
    let whole = id(1);
    sim.world.spawn_with_id(whole).unwrap();
    let parts = vec![
        cube(Vec3::new(-0.5, 0.0, 0.0), 0.5),
        cube(Vec3::new(0.5, 0.0, 0.0), 0.5),
    ];
    sim.insert(
        whole,
        Vec3::new(0.0, 3.0, 0.0),
        body(),
        Collider::new(ColliderShape::Compound { parts }),
    );
    sim.world
        .insert(
            whole,
            Velocity {
                linear: Vec3::new(4.0, 2.0, 0.0),
                angular: Vec3::ZERO,
            },
        )
        .unwrap();
    sim.step(30);
    let at = sim.pos(whole);
    let velocity = *sim.world.get::<Velocity>(whole).unwrap();
    sim.world.despawn(whole).unwrap();
    let pieces = [id(2), id(3)];
    for (piece, dx) in pieces.iter().zip([-0.5, 0.5]) {
        sim.world.spawn_with_id(*piece).unwrap();
        let points = cube(Vec3::ZERO, 0.5);
        sim.insert(
            *piece,
            at + Vec3::new(dx, 0.0, 0.0),
            body(),
            Collider::new(ColliderShape::ConvexHull { points }),
        );
        sim.world.insert(*piece, velocity).unwrap();
    }
    sim.step(ticks);
    (sim, pieces, velocity)
}

#[test]
fn pieces_spawned_mid_play_carry_the_bodys_momentum() {
    // No gravity: right after the split each piece (half the mass) moves at the whole's velocity,
    // so the pieces' momentum is the whole's.
    let (sim, pieces, before) = split_run(1, false, 1);
    for p in pieces {
        let v = sim.world.get::<Velocity>(p).unwrap().linear;
        assert!(
            (v - before.linear).length() < 1e-3,
            "{v} against {}",
            before.linear
        );
    }
    assert_eq!(sim.physics.body_count(), 2, "the whole's body is gone");
}

#[test]
fn a_mid_play_split_is_identical_across_threads_and_runs() {
    let hash = |threads| split_run(threads, true, 120).0.physics.state_hash();
    let one = hash(1);
    assert_eq!(one, hash(2), "2 threads");
    assert_eq!(one, hash(8), "8 threads");
    assert_eq!(one, hash(1), "a rerun");
}
