//! The Jolt world beside a Terra world: sync in, step, sync out, events. Deterministic: entities are
//! always visited in `EntityId` order and events are sorted.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use concordia_core::id::EntityId;
use concordia_core::math::{Quat, Ray, Vec3};
use concordia_scene::builtin::{Transform, Velocity};
use concordia_terra::World;
use jolt_sys as j;

use crate::components::{
    BodyKind, CharacterController, CharacterMotion, Collider, ColliderShape, RigidBody,
};
use crate::error::PhysicsError;
use crate::events::{ContactBegan, ContactEnded};
use crate::jolt::{BodyId, JoltCharacter, JoltWorld};
use crate::layers::LayerMask;
use crate::settings::PhysicsSettings;

/// What a tick could not simulate.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TickReport {
    /// Entities whose body or character was invalid, with the reason (they are skipped).
    pub rejected: Vec<(EntityId, String)>,
}

/// A raycast hit on an entity's body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    /// The entity hit.
    pub entity: EntityId,
    /// World-space hit point.
    pub point: Vec3,
    /// Surface normal at the hit.
    pub normal: Vec3,
    /// Distance from the ray origin.
    pub distance: f32,
}

/// blake3 of every body and character state, in `EntityId` order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateHash(pub [u8; 32]);

impl std::fmt::Display for StateHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

/// Authoring data a body was built from; a change rebuilds the body.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Source {
    pub(crate) body: RigidBody,
    pub(crate) collider: Collider,
    /// Only for static bodies (dynamic ones own their transform; kinematic ones are moved).
    fixed_at: Option<(Vec3, Quat)>,
}

#[derive(Debug)]
struct Tracked {
    id: BodyId,
    source: Source,
}

/// Jolt's world for one Terra world. Field order matters: characters drop before the world.
#[derive(Debug)]
pub struct PhysicsRuntime {
    characters: BTreeMap<EntityId, JoltCharacter>,
    bodies: BTreeMap<EntityId, Tracked>,
    users: BTreeMap<u64, EntityId>,
    next_user: u64,
    jolt: JoltWorld,
}

impl PhysicsRuntime {
    /// An empty physics world using up to `threads` threads per step.
    pub fn new(settings: PhysicsSettings, threads: NonZeroUsize) -> Result<Self, PhysicsError> {
        Ok(Self {
            characters: BTreeMap::new(),
            bodies: BTreeMap::new(),
            users: BTreeMap::new(),
            next_user: 0,
            jolt: JoltWorld::new(&settings, threads)?,
        })
    }

    /// A physics world mirroring `world`'s bodies without stepping (e.g. to raycast the edit world).
    pub fn snapshot(
        world: &World,
        settings: PhysicsSettings,
    ) -> Result<(Self, TickReport), PhysicsError> {
        let mut runtime = Self::new(settings, NonZeroUsize::MIN)?;
        let rejected = runtime.sync_bodies(world, None);
        Ok((runtime, TickReport { rejected }))
    }

    /// One fixed step: bodies in, simulate, characters move, transforms and events out.
    pub fn tick(&mut self, world: &mut World, dt: f32) -> Result<TickReport, PhysicsError> {
        let mut rejected = self.sync_bodies(world, Some(dt));
        rejected.extend(self.sync_characters(world));
        self.jolt.step(dt)?;
        self.move_characters(world, dt);
        self.write_back(world);
        self.raise_contacts(world);
        Ok(TickReport { rejected })
    }

    /// Bodies in the Jolt world.
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// The nearest body hit within `max_distance` on `layers`.
    pub fn raycast(&self, ray: &Ray, max_distance: f32, layers: LayerMask) -> Option<RayHit> {
        if !(max_distance.is_finite() && max_distance > 0.0) {
            return None;
        }
        let hit = self
            .jolt
            .raycast(ray.origin(), ray.dir() * max_distance, layers.0)?;
        Some(RayHit {
            entity: *self.users.get(&hit.user)?,
            point: hit.point,
            normal: hit.normal,
            distance: hit.fraction * max_distance,
        })
    }

    /// Hash of every body and character state, in `EntityId` order.
    pub fn state_hash(&self) -> StateHash {
        let mut hasher = blake3::Hasher::new();
        for (entity, tracked) in &self.bodies {
            hasher.update(entity.to_string().as_bytes());
            if let Some(s) = self.jolt.state(tracked.id) {
                let floats = [
                    s.position.to_array(),
                    s.linear.to_array(),
                    s.angular.to_array(),
                ];
                floats.iter().flatten().for_each(|x| {
                    hasher.update(&x.to_bits().to_le_bytes());
                });
                s.rotation.to_array().iter().for_each(|x| {
                    hasher.update(&x.to_bits().to_le_bytes());
                });
            }
        }
        for (entity, character) in &self.characters {
            hasher.update(entity.to_string().as_bytes());
            character.position().to_array().iter().for_each(|x| {
                hasher.update(&x.to_bits().to_le_bytes());
            });
        }
        StateHash(*hasher.finalize().as_bytes())
    }

    /// Jolt's body state, for restoring later with the same bodies.
    pub fn save_state(&self) -> Vec<u8> {
        self.jolt.save()
    }

    /// Restores [`save_state`](Self::save_state) output; the body set must be unchanged.
    pub fn restore_state(&mut self, data: &[u8]) -> Result<(), PhysicsError> {
        match self.jolt.restore(data) {
            true => Ok(()),
            false => Err(PhysicsError::Restore),
        }
    }

    /// Creates, rebuilds, moves and removes bodies to match the world.
    fn sync_bodies(&mut self, world: &World, dt: Option<f32>) -> Vec<(EntityId, String)> {
        let wanted: BTreeMap<EntityId, (Transform, Source)> = world
            .entity_ids_sorted()
            .into_iter()
            .filter_map(|id| source_of(world, id).map(|s| (id, s)))
            .collect();
        let gone: Vec<EntityId> = self
            .bodies
            .iter()
            .filter(|(id, t)| wanted.get(id).is_none_or(|(_, s)| *s != t.source))
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            if let Some(tracked) = self.bodies.remove(&id) {
                self.jolt.remove(tracked.id);
            }
        }
        let mut rejected = Vec::new();
        for (entity, (transform, source)) in wanted {
            match self.bodies.get(&entity) {
                Some(tracked) => self.drive_kinematic(tracked.id, &source, &transform, dt),
                None => match self.create_body(world, entity, &transform, source) {
                    Ok(()) => {}
                    Err(reason) => rejected.push((entity, reason)),
                },
            }
        }
        rejected
    }

    fn drive_kinematic(&mut self, id: BodyId, source: &Source, t: &Transform, dt: Option<f32>) {
        if let (BodyKind::Kinematic, Some(dt)) = (source.body.kind, dt) {
            self.jolt.move_kinematic(id, t.translation, t.rotation, dt);
        }
    }

    fn create_body(
        &mut self,
        world: &World,
        entity: EntityId,
        t: &Transform,
        source: Source,
    ) -> Result<(), String> {
        validate(&source, t)?;
        let user = self.next_user;
        let velocity = world.get::<Velocity>(entity).copied().unwrap_or_default();
        let desc = body_desc(user, t, &source, &velocity);
        let id = self
            .jolt
            .add(&desc)
            .ok_or_else(|| "Jolt rejected the body (world full?)".to_owned())?;
        self.next_user += 1;
        self.users.insert(user, entity);
        self.bodies.insert(entity, Tracked { id, source });
        Ok(())
    }

    fn sync_characters(&mut self, world: &World) -> Vec<(EntityId, String)> {
        let wanted: BTreeMap<EntityId, (Vec3, CharacterController)> = world
            .entity_ids_sorted()
            .into_iter()
            .filter_map(|id| {
                let t = world.get::<Transform>(id)?;
                let c = world.get::<CharacterController>(id)?;
                Some((id, (t.translation, c.clone())))
            })
            .collect();
        self.characters.retain(|id, _| wanted.contains_key(id));
        let mut rejected = Vec::new();
        for (entity, (at, controller)) in wanted {
            if self.characters.contains_key(&entity) {
                continue;
            }
            match self.create_character(entity, at, &controller) {
                Some(character) => {
                    self.characters.insert(entity, character);
                }
                None => rejected.push((
                    entity,
                    format!("invalid character controller {controller:?}"),
                )),
            }
        }
        rejected
    }

    /// A Jolt character whose inner body (seen only by sensors) reports contacts as `entity`.
    fn create_character(
        &mut self,
        entity: EntityId,
        at: Vec3,
        c: &CharacterController,
    ) -> Option<JoltCharacter> {
        let desc = j::CjCharacterDesc {
            height: c.height,
            radius: c.radius,
            max_slope_radians: c.max_slope_deg.to_radians(),
            step_height: c.step_height,
            position: at.to_array(),
            layer: c.layer.index()?,
            user: self.next_user,
        };
        let character = self.jolt.character(&desc)?;
        self.users.insert(self.next_user, entity);
        self.next_user += 1;
        Some(character)
    }

    fn move_characters(&mut self, world: &World, dt: f32) {
        let Self {
            characters, jolt, ..
        } = self;
        for (entity, character) in characters.iter_mut() {
            let motion = world
                .get::<CharacterMotion>(*entity)
                .copied()
                .unwrap_or_default();
            jolt.move_character(character, motion.velocity, dt);
        }
    }

    fn write_back(&self, world: &mut World) {
        for (entity, tracked) in &self.bodies {
            if tracked.source.body.kind != BodyKind::Dynamic {
                continue;
            }
            let Some(state) = self.jolt.state(tracked.id) else {
                continue;
            };
            if let Some(t) = world.get_mut::<Transform>(*entity) {
                t.translation = state.position;
                t.rotation = state.rotation;
            }
            let velocity = Velocity {
                linear: state.linear,
                angular: state.angular,
            };
            match world.get_mut::<Velocity>(*entity) {
                Some(v) => *v = velocity,
                None => {
                    let _ = world.insert(*entity, velocity);
                }
            }
        }
        for (entity, character) in &self.characters {
            if let Some(t) = world.get_mut::<Transform>(*entity) {
                t.translation = character.position();
            }
        }
    }

    /// Raises this step's contact changes as signals on both entities, in `(a, b)` id order.
    fn raise_contacts(&mut self, world: &mut World) {
        let mut contacts: Vec<(EntityId, EntityId, bool, bool)> = self
            .jolt
            .drain_events()
            .into_iter()
            .filter_map(|e| {
                let (x, y) = (*self.users.get(&e.a)?, *self.users.get(&e.b)?);
                Some((x.min(y), x.max(y), e.started, e.sensor))
            })
            .collect();
        contacts.sort();
        for (a, b, started, sensor) in contacts {
            for (on, other) in [(a, b), (b, a)] {
                if started {
                    world.signal(on, ContactBegan { other, sensor });
                } else {
                    world.signal(on, ContactEnded { other, sensor });
                }
            }
        }
    }
}

pub(crate) fn source_of(world: &World, id: EntityId) -> Option<(Transform, Source)> {
    let t = *world.get::<Transform>(id)?;
    let body = world.get::<RigidBody>(id)?.clone();
    let collider = world.get::<Collider>(id)?.clone();
    let fixed_at = (body.kind == BodyKind::Static).then_some((t.translation, t.rotation));
    Some((
        t,
        Source {
            body,
            collider,
            fixed_at,
        },
    ))
}

pub(crate) fn validate(source: &Source, t: &Transform) -> Result<(), String> {
    let positive = |name: &str, x: f32| match x.is_finite() && x > 0.0 {
        true => Ok(()),
        false => Err(format!("{name} must be a positive number, got {x}")),
    };
    match &source.collider.shape {
        ColliderShape::Box { half_extents } => half_extents
            .to_array()
            .iter()
            .try_for_each(|x| positive("box half_extents", *x)),
        ColliderShape::Sphere { radius } => positive("sphere radius", *radius),
        ColliderShape::Capsule {
            half_height,
            radius,
        }
        | ColliderShape::Cylinder {
            half_height,
            radius,
        } => positive("half_height", *half_height).and(positive("radius", *radius)),
    }?;
    if source.collider.layer.index().is_none() {
        return Err(format!(
            "collider layer {} is not in 0..=15",
            source.collider.layer.0
        ));
    }
    if !t.translation.is_finite() || !t.rotation.is_finite() {
        return Err("transform is not finite".to_owned());
    }
    Ok(())
}

fn body_desc(user: u64, t: &Transform, s: &Source, v: &Velocity) -> j::CjBodyDesc {
    let (shape, dims) = match s.collider.shape {
        ColliderShape::Box { half_extents } => {
            (j::CjShapeKind::CJ_SHAPE_BOX, half_extents.to_array())
        }
        ColliderShape::Sphere { radius } => (j::CjShapeKind::CJ_SHAPE_SPHERE, [radius, 0.0, 0.0]),
        ColliderShape::Capsule {
            half_height,
            radius,
        } => (j::CjShapeKind::CJ_SHAPE_CAPSULE, [half_height, radius, 0.0]),
        ColliderShape::Cylinder {
            half_height,
            radius,
        } => (
            j::CjShapeKind::CJ_SHAPE_CYLINDER,
            [half_height, radius, 0.0],
        ),
    };
    let motion = match s.body.kind {
        BodyKind::Static => j::CjMotion::CJ_MOTION_STATIC,
        BodyKind::Kinematic => j::CjMotion::CJ_MOTION_KINEMATIC,
        BodyKind::Dynamic => j::CjMotion::CJ_MOTION_DYNAMIC,
    };
    j::CjBodyDesc {
        user,
        shape,
        dims,
        offset: s.collider.offset.to_array(),
        position: t.translation.to_array(),
        rotation: t.rotation.to_array(),
        linear_velocity: v.linear.to_array(),
        angular_velocity: v.angular.to_array(),
        motion,
        layer: s.collider.layer.0,
        sensor: s.collider.is_sensor,
        ccd: s.body.ccd,
        mass: s.body.mass.unwrap_or(0.0),
        friction: s.body.friction,
        restitution: s.body.restitution,
        linear_damping: s.body.linear_damping,
        angular_damping: s.body.angular_damping,
        gravity_scale: s.body.gravity_scale,
    }
}
