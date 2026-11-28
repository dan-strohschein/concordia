//! `.breakable.ron`: the fracture recipe, the source of truth (the pieces are derived data).

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::{Pattern, StructureSpec};

/// A breakable's file extension.
pub const EXTENSION: &str = ".breakable.ron";

/// Bumped when the cutter's output changes (part of the pieces' cache key).
pub const FRACTURE_VERSION: u32 = 1;

/// How to break a mesh: written by `fracture.generate`, read back to rebuild its pieces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Breakable {
    /// The source mesh ("asset:<id>#mesh/…").
    pub source: String,
    /// How it is divided.
    pub pattern: Pattern,
    /// The pattern's seed.
    pub seed: u64,
    /// Metres per UV unit on interior faces.
    pub metres_per_tile: f32,
    /// The interior faces' material ("asset:<id>" or "builtin:material/…"); none uses the source's.
    pub interior_material: Option<String>,
    /// What holds the pieces together: density, bond strength, cluster levels and overrides.
    pub structure: StructureSpec,
}
