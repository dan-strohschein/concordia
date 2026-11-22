//! Physics for Concordia: Jolt-backed rigid bodies, colliders, characters and raycasts, synced with
//! Terra each fixed tick and deterministic across runs, thread counts and spawn order (ADR-0041).
//!
//! Authoring data (`RigidBody`, `Collider`, `CharacterController`) lives in the scene; the Jolt world
//! ([`PhysicsRuntime`]) lives beside the play world and is rebuilt for every play session (ADR-0032).

mod components;
mod error;
mod events;
mod jolt;
mod layers;
mod module;
mod runtime;
mod settings;
mod solids;

pub use components::{
    BodyKind, CharacterController, CharacterMotion, Collider, ColliderShape, RigidBody,
};
pub use error::PhysicsError;
pub use events::{ContactBegan, ContactEnded};
pub use layers::{LayerMask, PhysicsLayer};
pub use module::{
    PhysicsModule, PhysicsQueries, PhysicsRaycast, RaycastHit, RaycastParams, RaycastResult,
};
pub use runtime::{PhysicsRuntime, RayHit, StateHash, TickReport};
pub use settings::PhysicsSettings;
pub use solids::{StaticSolid, static_solids};
