//! The `Traffic` trait (ADR-0070): a template's entities drive the lanes.

use concordia_reflect::Reflect;
use concordia_scene::builtin::Transform;
use concordia_terra_sim::{EntityTrait, TemplateBuilder, TemplateError};
use serde::{Deserialize, Serialize};

use super::components::Vehicle;
use super::steps::DRIVE;

/// Trait: a vehicle (adds `Transform` and a `Vehicle`). Add a mesh, and a kinematic `RigidBody`
/// and `Collider` for a body physics characters bump into.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct Traffic {
    /// How it drives, and where to.
    pub vehicle: Vehicle,
}

impl EntityTrait for Traffic {
    fn build(&self, template: &mut TemplateBuilder<'_>) -> Result<(), TemplateError> {
        template.add(Transform::default())?;
        template.add(self.vehicle.clone())
    }

    fn requires(&self) -> Vec<String> {
        vec![DRIVE.to_owned()]
    }
}
