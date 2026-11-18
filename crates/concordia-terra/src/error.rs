//! ECS errors.

use concordia_core::id::{EntityId, IdError};
use concordia_reflect::ReflectError;

/// A world operation failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EcsError {
    /// An entity with this id already exists.
    #[error("entity {0} already exists")]
    DuplicateId(EntityId),
    /// No entity has this id.
    #[error("no entity {0}")]
    NoSuchEntity(EntityId),
    /// The name is not in the type registry.
    #[error("component {name:?} is not registered")]
    ComponentNotRegistered {
        /// The requested name.
        name: String,
    },
    /// The type is registered but not as a component.
    #[error(
        "type {name:?} is registered but is not a component; register it with register_component"
    )]
    NotAComponent {
        /// The type's name.
        name: String,
    },
    /// The entity lacks the component.
    #[error("entity {entity} has no {component} component")]
    ComponentMissing {
        /// The entity.
        entity: EntityId,
        /// The component's stable name.
        component: String,
    },
    /// Two different Rust types share a component name.
    #[error("component name {name:?} is used by two different types")]
    NameCollision {
        /// The contested name.
        name: &'static str,
    },
    /// A dynamic value had a different type than expected.
    #[error("expected a {expected} value, got {found}")]
    TypeMismatch {
        /// Expected type name.
        expected: &'static str,
        /// Actual type name.
        found: &'static str,
    },
    /// The entity table is full (2^32 slots).
    #[error("too many entities")]
    TooManyEntities,
    /// A value could not be built, serialized or hashed.
    #[error(transparent)]
    Reflect(#[from] ReflectError),
    /// An id source failed.
    #[error(transparent)]
    Id(#[from] IdError),
}

/// A query's access pattern is invalid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QueryError {
    /// A component appears more than once in the query.
    #[error("query accesses {component} more than once; each component may appear at most once")]
    Conflict {
        /// The repeated component's stable name.
        component: &'static str,
    },
}
