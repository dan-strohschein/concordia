//! Where breakables' pieces and structure come from.

use std::collections::BTreeMap;
use std::sync::Arc;

use concordia_assets::AssetRef;
use concordia_fracture::{Fracture, Structure};

use crate::DestructionError;

/// A breakable's pieces and the structure that holds them.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    /// The pieces.
    pub fracture: Fracture,
    /// Bonds, masses and cluster levels.
    pub structure: Structure,
    /// How its pieces are drawn (none: not drawn, as without a renderer).
    pub looks: Option<PieceLooks>,
}

/// How a breakable's pieces are drawn: its baked meshes and the interior's material (M8 8.5b).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PieceLooks {
    /// Every piece's surface faces (none: there are none).
    pub surface: Option<AssetRef>,
    /// Every piece's interior faces (none: there are none, as with one piece).
    pub interior: Option<AssetRef>,
    /// The interior's material; none uses the breakable's own.
    pub interior_material: Option<AssetRef>,
}

/// Loads breakables by recipe path.
pub trait BreakableSource: Send + Sync {
    /// The pieces and structure of the recipe at `recipe`.
    fn load(&self, recipe: &str) -> Result<Arc<Loaded>, DestructionError>;
}

/// Breakables held in memory (tests, and play sessions that loaded them up front).
#[derive(Debug, Clone, Default)]
pub struct MemorySource(BTreeMap<String, Arc<Loaded>>);

impl MemorySource {
    /// This source plus `loaded` under `recipe`.
    pub fn with(mut self, recipe: &str, loaded: Loaded) -> Self {
        self.0.insert(recipe.to_owned(), Arc::new(loaded));
        self
    }
}

impl BreakableSource for MemorySource {
    fn load(&self, recipe: &str) -> Result<Arc<Loaded>, DestructionError> {
        self.0
            .get(recipe)
            .cloned()
            .ok_or_else(|| DestructionError::Load {
                recipe: recipe.to_owned(),
                why: "not loaded".into(),
            })
    }
}
