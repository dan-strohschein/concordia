//! Canonical value hashing (M0 phase 0.2).

mod common;

use common::{Health, Mode, Zoo};
use concordia_reflect::{DynReflect, Reflect, canonical_hash};
use serde::{Deserialize, Serialize};

/// Same fields as `Health`, declared in the other order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
struct HealthReordered {
    /// Maximum.
    max: f32,
    /// Current.
    hp: f32,
}

fn digest(value: &dyn DynReflect) -> String {
    let mut hasher = blake3::Hasher::new();
    canonical_hash(value, &mut hasher).unwrap();
    hasher.finalize().to_hex().to_string()
}

#[test]
fn canonical_hash_equal_for_equal_values() {
    let a = Health { hp: 1.5, max: 10.0 };
    assert_eq!(digest(&a), digest(&a.clone()));
    assert_ne!(digest(&a), digest(&Health { hp: 1.5, max: 11.0 }));
}

#[test]
fn canonical_hash_distinguishes_neg_zero_and_field_order_invariant() {
    let zero = Health { hp: 0.0, max: 1.0 };
    let neg_zero = Health { hp: -0.0, max: 1.0 };
    assert_ne!(digest(&zero), digest(&neg_zero));
    assert_eq!(
        digest(&zero),
        digest(&HealthReordered { max: 1.0, hp: 0.0 })
    );
}

#[test]
fn canonical_hash_distinguishes_variants_and_one_ulp() {
    assert_ne!(digest(&Mode::Walk(1.0)), digest(&Mode::Jump(1.0, 0.0)));
    let next = f32::from_bits(1.0f32.to_bits() + 1);
    assert_ne!(digest(&Mode::Walk(1.0)), digest(&Mode::Walk(next)));
}

#[test]
fn canonical_hash_stable_snapshot() {
    let value = Zoo {
        name: common::Name("snap".into()),
        health: Health { hp: 3.0, max: 9.0 },
        mode: Mode::Patrol {
            points: vec![concordia_core::math::Vec3::new(1.0, 2.0, 3.0)],
            looping: true,
        },
        tags: vec!["a".into(), "b".into()],
        stats: [("speed".to_string(), 2.5)].into_iter().collect(),
        target: None,
        offset: Some(concordia_core::math::Vec2::new(0.5, -0.5)),
        pair: common::Pair(-4, "four".into()),
        marker: common::Marker,
        grid: [1, 2, 3],
        tuple: (7, false),
        cache: 99,
    };
    insta::assert_snapshot!(digest(&value));
}
