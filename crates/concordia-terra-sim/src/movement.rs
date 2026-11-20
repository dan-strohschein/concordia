//! The built-in `Movement` trait: constant velocity, integrated by the core
//! `integrate_velocity` system.

use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use concordia_scene::builtin::{Transform, Velocity};
use serde::{Deserialize, Serialize};

use crate::error::TemplateError;
use crate::traits::{EntityTrait, TemplateBuilder};

/// Trait: moves at a constant velocity (adds `Transform` and `Velocity`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Movement {
    /// Meters per second, world space.
    pub velocity: Vec3,
}

impl EntityTrait for Movement {
    fn build(&self, template: &mut TemplateBuilder<'_>) -> Result<(), TemplateError> {
        template.add(Transform::default())?;
        template.add(Velocity {
            linear: self.velocity,
            angular: Vec3::ZERO,
        })
    }

    fn requires(&self) -> Vec<String> {
        vec!["integrate_velocity".to_owned()]
    }
}
