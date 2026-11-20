//! In-memory scene and prefab documents.

use std::collections::BTreeMap;

use concordia_core::id::{EntityId, PrefabId, SceneId};
use concordia_reflect::DynReflect;

/// The scene format version this engine reads and writes.
pub const FORMAT_VERSION: u32 = 1;

/// Scene identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneMeta {
    /// Stable scene id.
    pub id: SceneId,
    /// Display name.
    pub name: String,
}

/// One entity: its stable id and components by stable name.
#[derive(Debug, Clone)]
pub struct EntityDoc {
    /// Stable id.
    pub id: EntityId,
    /// Component values by stable name.
    pub components: BTreeMap<String, Box<dyn DynReflect>>,
}

impl PartialEq for EntityDoc {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.components.len() == other.components.len()
            && self
                .components
                .iter()
                .zip(&other.components)
                .all(|((a, x), (b, y))| a == b && x.eq_dyn(y.as_ref()))
    }
}

/// A `.scene.ron` document.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneDoc {
    /// Format version (always [`FORMAT_VERSION`] once read).
    pub format: u32,
    /// Stable scene id.
    pub id: SceneId,
    /// Display name.
    pub name: String,
    /// Entities (written sorted by id).
    pub entities: Vec<EntityDoc>,
}

impl SceneDoc {
    /// The scene's identity.
    pub fn meta(&self) -> SceneMeta {
        SceneMeta {
            id: self.id,
            name: self.name.clone(),
        }
    }
}

/// A `.prefab.ron` document.
#[derive(Debug, Clone, PartialEq)]
pub struct PrefabDoc {
    /// Format version.
    pub format: u32,
    /// Stable prefab id.
    pub id: PrefabId,
    /// Display name.
    pub name: String,
    /// The root entity (must be one of `entities`).
    pub root: EntityId,
    /// Entities (written sorted by id).
    pub entities: Vec<EntityDoc>,
}

/// A `.template.ron` document (M2 phase 2.0c): Terra traits with their parameters, and
/// component values, by stable name.
#[derive(Debug, Clone)]
pub struct TemplateDoc {
    /// Format version (always [`FORMAT_VERSION`] once read).
    pub format: u32,
    /// What the template is for.
    pub description: String,
    /// Trait parameters by trait name (any registered reflected type).
    pub traits: BTreeMap<String, Box<dyn DynReflect>>,
    /// Component values by component name.
    pub components: BTreeMap<String, Box<dyn DynReflect>>,
}

impl PartialEq for TemplateDoc {
    fn eq(&self, other: &Self) -> bool {
        let same = |a: &BTreeMap<String, Box<dyn DynReflect>>,
                    b: &BTreeMap<String, Box<dyn DynReflect>>| {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|((k, x), (l, y))| k == l && x.eq_dyn(y.as_ref()))
        };
        self.format == other.format
            && self.description == other.description
            && same(&self.traits, &other.traits)
            && same(&self.components, &other.components)
    }
}
