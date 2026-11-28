//! Safe RAII wrappers over jolt-sys. The only module with `unsafe`.

use std::num::NonZeroUsize;

use concordia_core::math::{Quat, Vec3};
use jolt_sys as j;

use crate::error::PhysicsError;
use crate::settings::PhysicsSettings;

/// A Jolt body id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct BodyId(u32);

/// Position, rotation and velocities of a body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BodyState {
    pub(crate) position: Vec3,
    pub(crate) rotation: Quat,
    pub(crate) linear: Vec3,
    pub(crate) angular: Vec3,
}

/// A raycast hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Hit {
    pub(crate) user: u64,
    pub(crate) fraction: f32,
    pub(crate) point: Vec3,
    pub(crate) normal: Vec3,
}

/// A Jolt physics world (owned; destroyed on drop).
#[derive(Debug)]
pub(crate) struct JoltWorld(*mut j::CjWorld);

// SAFETY: the shim world has no thread affinity; Rust's &/&mut rules serialize access to it.
unsafe impl Send for JoltWorld {}

impl Drop for JoltWorld {
    fn drop(&mut self) {
        // SAFETY: created by cj_world_create and destroyed exactly once.
        unsafe { j::cj_world_destroy(self.0) };
    }
}

impl JoltWorld {
    pub(crate) fn new(
        settings: &PhysicsSettings,
        threads: NonZeroUsize,
    ) -> Result<Self, PhysicsError> {
        j::init().map_err(|e| PhysicsError::World(e.to_string()))?;
        let desc = j::CjWorldDesc {
            max_bodies: settings.max_bodies,
            threads: u32::try_from(threads.get()).unwrap_or(u32::MAX),
            gravity: settings.gravity.to_array(),
            collide: settings.matrix(),
        };
        // SAFETY: desc is a valid, fully initialized struct for the duration of the call.
        let world = unsafe { j::cj_world_create(&desc) };
        match world.is_null() {
            true => Err(PhysicsError::World(format!(
                "max_bodies must be 1..=65536 (got {}) and threads 1..=64 (got {})",
                settings.max_bodies,
                threads.get()
            ))),
            false => Ok(Self(world)),
        }
    }

    pub(crate) fn add(&mut self, desc: &j::CjBodyDesc) -> Option<BodyId> {
        // SAFETY: valid world and desc; the shim validates the desc's contents.
        let id = unsafe { j::cj_body_add(self.0, desc) };
        (id != j::CJ_NO_BODY).then_some(BodyId(id))
    }

    /// Adds a soft body (the desc's arrays must outlive the call).
    pub(crate) fn add_soft(&mut self, desc: &j::CjSoftBodyDesc) -> Option<BodyId> {
        // SAFETY: valid world and desc; the shim validates lengths, indices and values.
        let id = unsafe { j::cj_soft_body_add(self.0, desc) };
        (id != j::CJ_NO_BODY).then_some(BodyId(id))
    }

    /// A soft body's vertices in world space (empty if `body` isn't one).
    pub(crate) fn soft_positions(&self, body: BodyId) -> Vec<Vec3> {
        // SAFETY: valid world; a null `out` with no room only counts.
        let n = unsafe { j::cj_soft_body_positions(self.0, body.0, std::ptr::null_mut(), 0) };
        let mut out = vec![0.0f32; 3 * n];
        // SAFETY: `out` has room for `n` vertices.
        unsafe { j::cj_soft_body_positions(self.0, body.0, out.as_mut_ptr(), n) };
        out.as_chunks::<3>()
            .0
            .iter()
            .map(|p| Vec3::from_array(*p))
            .collect()
    }

    pub(crate) fn remove(&mut self, body: BodyId) {
        // SAFETY: valid world; unknown ids are ignored by the shim.
        unsafe { j::cj_body_remove(self.0, body.0) };
    }

    pub(crate) fn state(&self, body: BodyId) -> Option<BodyState> {
        let mut out = j::CjBodyState {
            position: [0.0; 3],
            rotation: [0.0; 4],
            linear_velocity: [0.0; 3],
            angular_velocity: [0.0; 3],
        };
        // SAFETY: valid world and out pointer.
        let found = unsafe { j::cj_body_state(self.0, body.0, &mut out) };
        found.then(|| BodyState {
            position: Vec3::from_array(out.position),
            rotation: Quat::from_array(out.rotation),
            linear: Vec3::from_array(out.linear_velocity),
            angular: Vec3::from_array(out.angular_velocity),
        })
    }

    pub(crate) fn move_kinematic(&mut self, body: BodyId, position: Vec3, rotation: Quat, dt: f32) {
        let (p, r) = (position.to_array(), rotation.to_array());
        // SAFETY: valid world; arrays live for the call.
        unsafe { j::cj_body_move_kinematic(self.0, body.0, p.as_ptr(), r.as_ptr(), dt) };
    }

    pub(crate) fn step(&mut self, dt: f32) -> Result<(), PhysicsError> {
        // SAFETY: valid world.
        match unsafe { j::cj_world_step(self.0, dt, 1) } {
            0 => Ok(()),
            bits => Err(PhysicsError::Step(bits)),
        }
    }

    /// Every pending contact event.
    pub(crate) fn drain_events(&mut self) -> Vec<j::CjContactEvent> {
        // SAFETY: valid world.
        let pending = unsafe { j::cj_world_pending_events(self.0) };
        let mut out = vec![
            j::CjContactEvent {
                a: 0,
                b: 0,
                started: false,
                sensor: false,
                strength: 0.0,
                point: [0.0; 3],
            };
            pending
        ];
        // SAFETY: out has room for `pending` events.
        let n = unsafe { j::cj_world_drain_events(self.0, out.as_mut_ptr(), out.len()) };
        out.truncate(n);
        out
    }

    pub(crate) fn raycast(&self, origin: Vec3, dir_len: Vec3, mask: u16) -> Option<Hit> {
        let (o, d) = (origin.to_array(), dir_len.to_array());
        let mut out = j::CjRayHit {
            user: 0,
            fraction: 0.0,
            point: [0.0; 3],
            normal: [0.0; 3],
        };
        // SAFETY: valid world, arrays and out pointer for the call.
        let hit = unsafe { j::cj_raycast(self.0, o.as_ptr(), d.as_ptr(), mask, &mut out) };
        hit.then(|| Hit {
            user: out.user,
            fraction: out.fraction,
            point: Vec3::from_array(out.point),
            normal: Vec3::from_array(out.normal),
        })
    }

    pub(crate) fn save(&self) -> Vec<u8> {
        // SAFETY: a null buffer only asks for the size.
        let size = unsafe { j::cj_world_save(self.0, std::ptr::null_mut(), 0) };
        let mut out = vec![0; size];
        // SAFETY: out holds `size` bytes, the size the shim reported.
        unsafe { j::cj_world_save(self.0, out.as_mut_ptr(), out.len()) };
        out
    }

    pub(crate) fn restore(&mut self, data: &[u8]) -> bool {
        // SAFETY: valid world; the slice lives for the call.
        unsafe { j::cj_world_restore(self.0, data.as_ptr(), data.len()) }
    }

    pub(crate) fn character(&mut self, desc: &j::CjCharacterDesc) -> Option<JoltCharacter> {
        // SAFETY: valid world and desc; the shim validates the contents.
        let character = unsafe { j::cj_character_create(self.0, desc) };
        (!character.is_null()).then_some(JoltCharacter(character))
    }

    pub(crate) fn move_character(
        &mut self,
        character: &mut JoltCharacter,
        velocity: Vec3,
        dt: f32,
    ) {
        let v = velocity.to_array();
        // SAFETY: the character was created in this world, which outlives it (runtime field order).
        unsafe { j::cj_character_move(self.0, character.0, v.as_ptr(), dt) };
    }
}

/// A virtual character (owned; destroyed on drop, before its world).
#[derive(Debug)]
pub(crate) struct JoltCharacter(*mut j::CjCharacter);

// SAFETY: as for JoltWorld.
unsafe impl Send for JoltCharacter {}

impl Drop for JoltCharacter {
    fn drop(&mut self) {
        // SAFETY: created by cj_character_create and destroyed exactly once.
        unsafe { j::cj_character_destroy(self.0) };
    }
}

impl JoltCharacter {
    pub(crate) fn position(&self) -> Vec3 {
        let mut out = [0.0; 3];
        // SAFETY: valid character and out array.
        unsafe { j::cj_character_position(self.0, out.as_mut_ptr()) };
        Vec3::from_array(out)
    }
}

/// The triangles of the convex hull of `points`, as Jolt builds it for a `ConvexHull` collider.
pub fn hull_triangles(points: &[Vec3]) -> Result<Vec<[Vec3; 3]>, PhysicsError> {
    j::init().map_err(|e| PhysicsError::World(e.to_string()))?;
    let flat: Vec<f32> = points.iter().flat_map(|p| p.to_array()).collect();
    let count =
        u32::try_from(points.len()).map_err(|_| PhysicsError::Hull("too many points".into()))?;
    // SAFETY: `flat` holds 3 floats per point; a null `out` with no room only counts.
    let n = unsafe { j::cj_convex_hull_triangles(flat.as_ptr(), count, std::ptr::null_mut(), 0) };
    if n == 0 {
        return Err(PhysicsError::Hull(format!("{} points", points.len())));
    }
    let mut out = vec![0.0f32; 9 * n];
    // SAFETY: `out` has room for `n` triangles of 9 floats.
    let written = unsafe { j::cj_convex_hull_triangles(flat.as_ptr(), count, out.as_mut_ptr(), n) };
    debug_assert_eq!(written, n);
    Ok(out
        .as_chunks::<9>()
        .0
        .iter()
        .map(|t| [0, 3, 6].map(|k| Vec3::new(t[k], t[k + 1], t[k + 2])))
        .collect())
}
