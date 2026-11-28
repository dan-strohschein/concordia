//! Jolt's soft body as a test reference (M8 8.0d, Q1): the cloth solver (M8 8.8) is checked
//! against it. Not for gameplay: cloth runs in `concordia-cloth`.

use std::num::NonZeroUsize;

use concordia_core::math::Vec3;
use jolt_sys as j;

use crate::error::PhysicsError;
use crate::jolt::{BodyId, JoltWorld};
use crate::settings::PhysicsSettings;

/// A cloth sheet for the reference: vertices at rest (world space), inverse masses (0 pins a
/// vertex), triangles, and the constraint settings.
#[derive(Debug, Clone, PartialEq)]
pub struct SoftSheet {
    /// Vertex positions.
    pub positions: Vec<Vec3>,
    /// Per-vertex inverse mass; 0 pins.
    pub inv_masses: Vec<f32>,
    /// Triangles.
    pub faces: Vec<[u32; 3]>,
    /// Stretch compliance (0: stiff).
    pub compliance: f32,
    /// Shear compliance.
    pub shear_compliance: f32,
    /// Bend compliance (`None`: no bending constraints).
    pub bend_compliance: Option<f32>,
    /// Solver iterations per step.
    pub iterations: u32,
    /// Gravity multiplier.
    pub gravity_factor: f32,
}

impl Default for SoftSheet {
    fn default() -> Self {
        Self {
            positions: Vec::new(),
            inv_masses: Vec::new(),
            faces: Vec::new(),
            compliance: 0.0,
            shear_compliance: 0.0,
            bend_compliance: None,
            iterations: 8,
            gravity_factor: 1.0,
        }
    }
}

/// One soft body in its own single-threaded Jolt world.
pub struct SoftReference {
    world: JoltWorld,
    body: BodyId,
}

impl SoftReference {
    /// The sheet in a new world with the default gravity.
    pub fn new(sheet: &SoftSheet) -> Result<Self, PhysicsError> {
        if sheet.inv_masses.len() != sheet.positions.len() {
            return Err(PhysicsError::Invalid(format!(
                "{} inverse masses for {} vertices",
                sheet.inv_masses.len(),
                sheet.positions.len()
            )));
        }
        let mut world = JoltWorld::new(&PhysicsSettings::default(), NonZeroUsize::MIN)?;
        let positions: Vec<f32> = sheet.positions.iter().flat_map(|p| p.to_array()).collect();
        let faces: Vec<u32> = sheet.faces.iter().flatten().copied().collect();
        let count =
            |n: usize| u32::try_from(n).map_err(|_| PhysicsError::Invalid("too large".into()));
        let desc = j::CjSoftBodyDesc {
            positions: positions.as_ptr(),
            inv_masses: sheet.inv_masses.as_ptr(),
            vertex_count: count(sheet.positions.len())?,
            faces: faces.as_ptr(),
            face_count: count(sheet.faces.len())?,
            compliance: sheet.compliance,
            shear_compliance: sheet.shear_compliance,
            bend_compliance: sheet.bend_compliance.unwrap_or(-1.0),
            iterations: sheet.iterations,
            gravity_factor: sheet.gravity_factor,
        };
        let body = world
            .add_soft(&desc)
            .ok_or_else(|| PhysicsError::Invalid("Jolt rejected the soft body (no faces, an index out of range, a degenerate face or a value that is not finite)".into()))?;
        Ok(Self { world, body })
    }

    /// Steps `ticks` times by `dt`.
    pub fn step(&mut self, ticks: u32, dt: f32) -> Result<(), PhysicsError> {
        (0..ticks).try_for_each(|_| self.world.step(dt))
    }

    /// The vertices now, in world space.
    pub fn positions(&self) -> Vec<Vec3> {
        self.world.soft_positions(self.body)
    }
}
