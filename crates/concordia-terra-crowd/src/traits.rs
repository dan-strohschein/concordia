//! The `Crowd` trait (ADR-0070): a template's entities walk with the crowd.

use concordia_nav::{CrowdAgent, NavAgent};
use concordia_reflect::Reflect;
use concordia_scene::builtin::Transform;
use concordia_terra_sim::{EntityTrait, TemplateBuilder, TemplateError};
use serde::{Deserialize, Serialize};

use crate::processors::MOVE;

/// Trait: a crowd agent (adds `Transform`, a `NavAgent` with no target and a `CrowdAgent`). Add a
/// `CharacterController` component for a physics body; without one it walks the navmesh surface.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct Crowd {
    /// Speed, sizes and distances.
    pub agent: NavAgent,
    /// The crowd's tuning.
    pub crowd: CrowdAgent,
}

impl EntityTrait for Crowd {
    fn build(&self, template: &mut TemplateBuilder<'_>) -> Result<(), TemplateError> {
        template.add(Transform::default())?;
        template.add(self.agent.clone())?;
        template.add(self.crowd.clone())
    }

    fn requires(&self) -> Vec<String> {
        vec![MOVE.to_owned()]
    }
}
