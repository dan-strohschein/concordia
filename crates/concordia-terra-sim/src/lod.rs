//! The built-in `Lod` trait (M2 phase 2.4b.1, ADR-0076): a template's simulation LOD profile,
//! shared by every entity spawned from it.

use concordia_reflect::Reflect;
use concordia_terra::{LOD, LodProfile};
use serde::{Deserialize, Serialize};

use crate::error::TemplateError;
use crate::traits::{EntityTrait, TemplateBuilder};

/// Trait: ticks less often far from every `LodViewer` (adds a shared `LodProfile`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Lod {
    /// Meters from the nearest viewer where levels 1, 2 and 3 start (ascending).
    pub distances: [f32; 3],
    /// Ticks between updates at levels 1, 2 and 3 (level 0 updates every tick).
    pub rates: [u32; 3],
    /// Meters an entity must come back inside a boundary before it regains detail.
    pub hysteresis: f32,
}

impl EntityTrait for Lod {
    fn build(&self, template: &mut TemplateBuilder<'_>) -> Result<(), TemplateError> {
        let profile =
            LodProfile::new(self.distances, self.rates, self.hysteresis).map_err(|e| {
                TemplateError::InvalidValue {
                    what: "trait Lod".to_owned(),
                    reason: e.to_string(),
                }
            })?;
        template.add_shared(profile)
    }

    fn requires(&self) -> Vec<String> {
        vec![LOD.to_owned()]
    }
}
