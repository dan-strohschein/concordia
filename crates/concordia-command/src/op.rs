//! Edit operations: serializable data (never closures), with component values as JSON.

use std::collections::BTreeMap;

use concordia_core::id::EntityId;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One change to an edit world.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum EditOp {
    /// Create an entity with the given id and components.
    Spawn {
        /// The new entity's id.
        id: EntityId,
        /// Component values by stable name.
        components: BTreeMap<String, Value>,
    },
    /// Remove an entity and all its components.
    Despawn {
        /// The entity.
        id: EntityId,
    },
    /// Insert or replace one component.
    SetComponent {
        /// The entity.
        id: EntityId,
        /// Component stable name.
        component: String,
        /// The new value.
        value: Value,
    },
    /// Remove one component (it must be present).
    RemoveComponent {
        /// The entity.
        id: EntityId,
        /// Component stable name.
        component: String,
    },
    /// Move a script (dynamic) component type to a new version: the registry takes the new
    /// description and every stored value is converted through reflect `migrate`. Undo restores
    /// the previous version and the exact previous values.
    MigrateType {
        /// Component stable name.
        component: String,
        /// The new version's `TypeDesc`, as JSON.
        to: Value,
    },
    /// Replace a project text file (M4 4.10c): `before` must match what it holds now (`None`:
    /// absent); `after` is the new text (`None`: delete it). Full texts, so undo is byte-exact.
    Document {
        /// Project-relative path (`/`-separated, inside the project).
        path: String,
        /// The text it holds now.
        before: Option<String>,
        /// The text it will hold.
        after: Option<String>,
    },
}

impl EditOp {
    /// The entity the op touches (`None` for a type migration, which touches every entity with
    /// the component).
    pub fn entity(&self) -> Option<EntityId> {
        match self {
            Self::Spawn { id, .. }
            | Self::Despawn { id }
            | Self::SetComponent { id, .. }
            | Self::RemoveComponent { id, .. } => Some(*id),
            Self::MigrateType { .. } | Self::Document { .. } => None,
        }
    }

    /// Label for an implicit single-op transaction.
    pub fn default_label(&self) -> &'static str {
        match self {
            Self::Spawn { .. } => "spawn",
            Self::Despawn { .. } => "despawn",
            Self::SetComponent { .. } => "set_component",
            Self::RemoveComponent { .. } => "remove_component",
            Self::MigrateType { .. } => "migrate_type",
            Self::Document { .. } => "document",
        }
    }
}

/// An op as applied, with the ops that undo it (computed at apply time).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppliedOp {
    /// The op.
    pub op: EditOp,
    /// Applying these in order reverts `op`.
    pub inverse: Vec<EditOp>,
}
