//! Builds worlds of bodies and steps them with a PhysicsRuntime.
#![allow(dead_code, missing_docs)]

use std::num::NonZeroUsize;

use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::math::{Quat, Vec3};
use concordia_physics::{
    BodyKind, Collider, ColliderShape, PhysicsRuntime, PhysicsSettings, RigidBody,
};
use concordia_scene::builtin::{Transform, Velocity};
use concordia_terra::World;

pub const DT: f32 = 1.0 / 60.0;

pub struct Sim {
    pub world: World,
    pub physics: PhysicsRuntime,
    ids: DeterministicIdGen,
}

impl Sim {
    pub fn new() -> Self {
        Self::with(PhysicsSettings::default(), 1)
    }

    pub fn with(settings: PhysicsSettings, threads: usize) -> Self {
        Self {
            world: World::new(),
            physics: PhysicsRuntime::new(settings, NonZeroUsize::new(threads).unwrap()).unwrap(),
            ids: DeterministicIdGen::new(11),
        }
    }

    pub fn spawn(&mut self, at: Vec3, body: RigidBody, collider: Collider) -> EntityId {
        let id = self.world.spawn(&mut self.ids).unwrap();
        self.insert(id, at, body, collider);
        id
    }

    pub fn insert(&mut self, id: EntityId, at: Vec3, body: RigidBody, collider: Collider) {
        let transform = Transform {
            translation: at,
            ..Transform::default()
        };
        self.world.insert(id, transform).unwrap();
        self.world.insert(id, body).unwrap();
        self.world.insert(id, collider).unwrap();
    }

    pub fn ground(&mut self) -> EntityId {
        self.spawn(
            Vec3::new(0.0, -0.5, 0.0),
            RigidBody::fixed(),
            Collider::new(ColliderShape::Box {
                half_extents: Vec3::new(30.0, 0.5, 30.0),
            }),
        )
    }

    pub fn ball(&mut self, at: Vec3, radius: f32) -> EntityId {
        self.spawn(
            at,
            RigidBody::dynamic(),
            Collider::new(ColliderShape::Sphere { radius }),
        )
    }

    pub fn cube(&mut self, at: Vec3) -> EntityId {
        self.spawn(
            at,
            RigidBody::dynamic(),
            Collider::new(ColliderShape::Box {
                half_extents: Vec3::splat(0.5),
            }),
        )
    }

    pub fn step(&mut self, ticks: u32) {
        for _ in 0..ticks {
            let report = self.physics.tick(&mut self.world, DT).unwrap();
            assert!(report.rejected.is_empty(), "{report:?}");
        }
    }

    pub fn pos(&self, id: EntityId) -> Vec3 {
        self.world.get::<Transform>(id).unwrap().translation
    }

    pub fn rot(&self, id: EntityId) -> Quat {
        self.world.get::<Transform>(id).unwrap().rotation
    }

    pub fn speed(&self, id: EntityId) -> f32 {
        self.world
            .get::<Velocity>(id)
            .map_or(0.0, |v| v.linear.length())
    }
}

pub fn body(kind: BodyKind) -> RigidBody {
    RigidBody {
        kind,
        ..RigidBody::default()
    }
}
