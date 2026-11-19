//! Entity templates: ready-made entities (a cube, a point light, ...) that modules contribute and
//! `entity.create` spawns, so agents and the editor share one list.

use std::collections::BTreeMap;

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A ready-made entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct EntityTemplate {
    /// Display name and the default entity name, e.g. "Point Light".
    pub name: String,
    /// What it is for, for agents and the editor.
    pub description: String,
    /// Component values by name.
    pub components: BTreeMap<String, Value>,
}

impl EntityTemplate {
    /// A template from its components.
    pub fn new<'a>(
        name: &str,
        description: &str,
        components: impl IntoIterator<Item = (&'a str, Value)>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            description: description.to_owned(),
            components: components
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect(),
        }
    }
}
