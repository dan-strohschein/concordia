//! Proptest strategies for the zoo (shared by the schema and wire tests).

use std::collections::BTreeMap;

use concordia_core::id::{DeterministicIdGen, EntityId, IdSource};
use concordia_core::math::{Vec2, Vec3};
use proptest::prelude::*;

use super::{Health, Marker, Mode, Name, Pair, Zoo};

pub fn finite32() -> impl Strategy<Value = f32> {
    any::<f32>().prop_filter("finite", |x| x.is_finite())
}

pub fn finite64() -> impl Strategy<Value = f64> {
    any::<f64>().prop_filter("finite", |x| x.is_finite())
}

pub fn vec3() -> impl Strategy<Value = Vec3> {
    (finite32(), finite32(), finite32()).prop_map(|(x, y, z)| Vec3::new(x, y, z))
}

pub fn mode() -> impl Strategy<Value = Mode> {
    prop_oneof![
        Just(Mode::Idle),
        finite32().prop_map(Mode::Walk),
        (finite32(), finite32()).prop_map(|(h, d)| Mode::Jump(h, d)),
        (prop::collection::vec(vec3(), 0..4), any::<bool>())
            .prop_map(|(points, looping)| Mode::Patrol { points, looping }),
    ]
}

pub fn entity_id() -> impl Strategy<Value = EntityId> {
    any::<u64>().prop_map(|seed| DeterministicIdGen::new(seed).next_id().unwrap())
}

prop_compose! {
    pub fn zoo()(
        name in ".*",
        // `Health.hp` declares range(0..=1000); serde does not enforce it, the schema does.
        hp in 0.0f32..=1000.0, max in finite32(),
        mode in mode(),
        tags in prop::collection::vec(".*", 0..4),
        stats in prop::collection::btree_map(".*", finite64(), 0..4),
        target in prop::option::of(entity_id()),
        offset in prop::option::of((finite32(), finite32()).prop_map(|(x, y)| Vec2::new(x, y))),
        pair in (any::<i32>(), ".*"),
        grid in any::<[u8; 3]>(),
        tuple in any::<(i32, bool)>(),
    ) -> Zoo {
        Zoo {
            name: Name(name), health: Health { hp, max }, mode, tags,
            stats: stats.into_iter().collect::<BTreeMap<_, _>>(), target, offset,
            pair: Pair(pair.0, pair.1), marker: Marker, grid, tuple, cache: 0,
        }
    }
}
