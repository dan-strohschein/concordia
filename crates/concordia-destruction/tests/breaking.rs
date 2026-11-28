//! Breaking at runtime on Jolt (M8 8.3). The breakable is a 2 × 1 × 1 slab sliced into four
//! 0.5 m slabs along x (cells 0–3, bonds 0–1, 1–2, 2–3, each 1 m² × strength 100 = 100). Impacts
//! are raised by a test system, so every run sees exactly the same blows.

mod common;

use common::*;
use concordia_core::id::EntityId;
use concordia_core::math::{Quat, Vec3};
use concordia_destruction::Fragment;
use concordia_physics::{BodyKind, Collider, ColliderShape, RigidBody};
use concordia_scene::builtin::{Transform, Velocity};

#[test]
fn an_unstrained_cluster_stays_one_body() {
    let mut world = run(&Run::default(), 30);
    let all = fragments(&mut world);
    assert_eq!(all.keys().collect::<Vec<_>>(), [&vec![0, 1, 2, 3]]);
    let id = all[&vec![0, 1, 2, 3]];
    assert_eq!(id, root(), "the breakable itself is the first fragment");
    let ColliderShape::Compound { parts } = &world.get::<Collider>(id).unwrap().shape else {
        panic!("one compound body of its pieces");
    };
    assert_eq!(parts.len(), 4);
    // Its mass comes from the structure: 2 m³ at 1000 kg/m³.
    let mass = world.get::<RigidBody>(id).unwrap().mass.unwrap();
    assert!((mass - 2000.0).abs() < 1e-2, "{mass}");
}

#[test]
fn a_weak_blow_damages_without_breaking() {
    let r = Run {
        blows: vec![Blow {
            strength: 60.0,
            ..middle(5)
        }],
        ..Run::default()
    };
    let mut world = run(&r, 20);
    let all = fragments(&mut world);
    assert_eq!(all.len(), 1, "60 < 100: still whole");
    let f = world.get::<Fragment>(root()).unwrap();
    assert!(
        f.damage.iter().any(|d| *d > 0.0),
        "the bond remembers the blow"
    );
    assert!(f.broken.is_empty());
}

#[test]
fn impulse_above_threshold_splits_into_connected_components() {
    let mut r = Run {
        blows: vec![middle(5)],
        ..Run::default()
    };
    let mut world = run(&r, 20);
    let all = fragments(&mut world);
    assert_eq!(
        all.keys().cloned().collect::<Vec<_>>(),
        [vec![0, 1], vec![2, 3]],
        "only the middle bond broke"
    );
    assert!(world.get::<Fragment>(root()).is_none(), "the whole is gone");
    for (pieces, id) in &all {
        let ColliderShape::Compound { parts } = &world.get::<Collider>(*id).unwrap().shape else {
            panic!("each part is a compound of its pieces");
        };
        assert_eq!(parts.len(), pieces.len());
        let mass = world.get::<RigidBody>(*id).unwrap().mass.unwrap();
        assert!((mass - 1000.0).abs() < 1e-2, "{mass}");
    }
    // A second blow on the same spot finds nothing left to break.
    r.blows.push(middle(10));
    assert_eq!(fragments(&mut run(&r, 20)).len(), 2);
}

#[test]
fn momentum_is_conserved_across_a_split() {
    // Spinning about z (a principal axis of the slab, so ω stays constant) while drifting.
    let (v, w) = (Vec3::new(1.0, 0.5, 0.0), Vec3::new(0.0, 0.0, 2.0));
    let r = Run {
        velocity: Velocity {
            linear: v,
            angular: w,
        },
        blows: vec![middle(5)],
        ..Run::default()
    };
    // Read the parts on the tick they appear, before physics moves them again.
    let mut before = r.blows[0].tick + 1;
    let mut world = loop {
        let mut world = run(&r, before as u32);
        if fragments(&mut world).len() == 2 {
            break world;
        }
        before += 1;
        assert!(before < 20, "the split never happened");
    };
    let parts = fragments(&mut world);
    // The whole's centre of mass in mesh space is (1, 0.5, 0.5); each half's is ±0.5 m along x.
    let rotation = world.get::<Transform>(parts[&vec![0, 1]]).unwrap().rotation;
    let (mut momentum, mut mass) = (Vec3::ZERO, 0.0);
    for (pieces, id) in &parts {
        let m = world.get::<RigidBody>(*id).unwrap().mass.unwrap();
        let vel = *world.get::<Velocity>(*id).unwrap();
        let dx = if pieces[0] == 0 { -0.5 } else { 0.5 };
        let expected = v + w.cross(rotation * Vec3::new(dx, 0.0, 0.0));
        assert!(
            (vel.linear - expected).length() < 1e-3,
            "{pieces:?}: {vel:?} vs {expected}"
        );
        assert!((vel.angular - w).length() < 1e-4, "{vel:?}");
        momentum += vel.linear * m;
        mass += m;
    }
    // Equal halves: the linear momentum is the whole's, and each part moves rigidly with it,
    // so the angular momentum is the whole's too.
    assert!(
        (momentum - v * mass).length() < 1e-2,
        "{momentum} vs {}",
        v * mass
    );
}

#[test]
fn anchored_components_stay_static() {
    let r = Run {
        gravity: true,
        anchors: vec![0],
        blows: vec![middle(5)],
        ..Run::default()
    };
    let mut world = run(&r, 90);
    let parts = fragments(&mut world);
    assert_eq!(parts.len(), 2);
    let (anchored, loose) = (parts[&vec![0, 1]], parts[&vec![2, 3]]);
    assert_eq!(
        world.get::<RigidBody>(anchored).unwrap().kind,
        BodyKind::Static
    );
    assert_eq!(
        world.get::<RigidBody>(loose).unwrap().kind,
        BodyKind::Dynamic
    );
    let at = |id| world.get::<Transform>(id).unwrap().translation;
    assert_eq!(at(anchored), START, "the anchored half never moved");
    assert!(
        at(loose).y < START.y - 1.0,
        "the loose half fell: {}",
        at(loose)
    );
}

#[test]
fn break_budget_defers_excess_breaks_to_later_ticks() {
    // One wide blow over every bond, with a budget of one break a tick.
    let r = Run {
        radius: 5.0,
        breaks_per_tick: 1,
        blows: vec![Blow {
            tick: 5,
            local: Vec3::new(1.0, 0.5, 0.5),
            strength: 1000.0,
        }],
        ..Run::default()
    };
    let counts: Vec<usize> = (6..=10).map(|t| fragments(&mut run(&r, t)).len()).collect();
    assert_eq!(counts.first(), Some(&1), "{counts:?}");
    assert_eq!(counts.last(), Some(&4), "{counts:?}");
    assert!(
        counts.windows(2).all(|w| w[1] <= w[0] + 1),
        "at most one more piece a tick: {counts:?}"
    );
}

/// A fragment's id, pieces and pose bits.
type Pose = (EntityId, Vec<u32>, [u32; 3], [u32; 4]);

#[test]
fn destruction_hash_is_identical_at_1_2_and_8_threads() {
    let hash = |threads| {
        let r = Run {
            threads,
            gravity: true,
            velocity: Velocity {
                linear: Vec3::new(0.5, 0.0, 0.0),
                angular: Vec3::new(0.0, 1.0, 0.0),
            },
            blows: vec![
                middle(5),
                Blow {
                    tick: 20,
                    local: Vec3::new(0.5, 0.5, 0.5),
                    strength: 500.0,
                },
            ],
            ..Run::default()
        };
        let mut world = run(&r, 90);
        let mut parts: Vec<Pose> = Vec::new();
        if let Ok(mut q) = world.query::<(&Fragment, &Transform)>() {
            q.for_each(&mut world, |id, (f, t)| {
                parts.push((
                    id,
                    f.pieces.clone(),
                    t.translation.to_array().map(f32::to_bits),
                    Quat::to_array(&t.rotation).map(f32::to_bits),
                ))
            });
        }
        parts.sort();
        assert!(
            parts.len() > 2,
            "both blows split something: {}",
            parts.len()
        );
        parts
    };
    let one = hash(1);
    assert_eq!(one, hash(2));
    assert_eq!(one, hash(8));
    assert_eq!(one, hash(1), "and a rerun");
}
