//! Shared test components.
#![allow(dead_code, missing_docs)]

use concordia_core::id::{DeterministicIdGen, EntityId, IdSource};
use concordia_reflect::{Reflect, TypeRegistry};
use concordia_terra::{World, register_component};
use serde::{Deserialize, Serialize};

/// Test component A.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct A(pub u32);

/// Test component B.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct B(pub f32);

/// Test component C.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct C(pub String);

/// Position.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub struct Pos {
    /// X.
    pub x: f32,
    /// Y.
    pub y: f32,
    /// Z.
    pub z: f32,
}

/// Velocity.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub struct Vel {
    /// X.
    pub x: f32,
    /// Y.
    pub y: f32,
    /// Z.
    pub z: f32,
}

/// Zero-sized marker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Marker;

/// Over-aligned component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[repr(align(64))]
pub struct Aligned {
    /// Payload.
    pub v: u64,
}

/// Deterministic ids for tests.
pub fn ids(n: usize) -> Vec<EntityId> {
    let mut source = DeterministicIdGen::new(1);
    (0..n).map(|_| source.next_id().unwrap()).collect()
}

/// A registry with the test components registered as components.
pub fn registry() -> TypeRegistry {
    let mut reg = TypeRegistry::new();
    register_component::<A>(&mut reg).unwrap();
    register_component::<B>(&mut reg).unwrap();
    register_component::<C>(&mut reg).unwrap();
    register_component::<Pos>(&mut reg).unwrap();
    register_component::<Vel>(&mut reg).unwrap();
    reg
}

/// A world with `n` entities carrying `Pos` and `Vel`.
pub fn moving_world(n: usize) -> World {
    let mut world = World::new();
    for (i, id) in ids(n).into_iter().enumerate() {
        let f = i as f32;
        world.spawn_with_id(id).unwrap();
        world
            .insert(
                id,
                Pos {
                    x: f,
                    y: 0.0,
                    z: -f,
                },
            )
            .unwrap();
        world
            .insert(
                id,
                Vel {
                    x: 1.0,
                    y: f * 0.001,
                    z: 0.5,
                },
            )
            .unwrap();
    }
    world
}
