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
    /// A value is out of range.
    #[error("{what} must be {expected}, got {value}")]
    InvalidValue {
        /// What was given.
        what: &'static str,
        /// What is allowed.
        expected: &'static str,
        /// The value.
        value: String,
    },
    /// A value could not be built, serialized or hashed.
    #[error(transparent)]
    Reflect(#[from] ReflectError),
    /// An id source failed.
    #[error(transparent)]
    Id(#[from] IdError),
    /// A runtime-described (script) type cannot be stored as asked.
    #[error("{component}: {message}")]
    Dynamic {
        /// The type's stable name.
        component: String,
        /// What is wrong.
        message: String,
    },
    /// A storage kind was chosen for a type the world already stores another way.
    #[error(
        "{component} is already stored as an ordinary component; register its storage kind before first use"
    )]
    StorageKindInUse {
        /// The component's stable name.
        component: &'static str,
    },
}

/// A query's access pattern is invalid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QueryError {
    /// A wire batch does not match the view (row count, unknown entity, wrong type).
    #[error("batch for {component} does not match: {message}")]
    BatchMismatch {
        /// The component's stable name.
        component: &'static str,
        /// What is wrong.
        message: String,
    },
    /// A wire column could not be decoded.
    #[error("batch for {component} is malformed: {message}")]
    Wire {
        /// The component's stable name.
        component: &'static str,
        /// The decoding error.
        message: String,
    },
    /// A component appears more than once in the query.
    #[error("query accesses {component} more than once; each component may appear at most once")]
    Conflict {
        /// The repeated component's stable name.
        component: &'static str,
    },
    /// A processor's query uses a component it did not declare.
    #[error(
        "the processor did not declare {component}; add reads::<{component}>() or writes::<{component}>()"
    )]
    Undeclared {
        /// The component's stable name.
        component: &'static str,
    },
    /// A processor's query writes a component it declared read-only.
    #[error(
        "the processor declared {component} read-only; declare writes::<{component}>() to change it"
    )]
    ReadOnly {
        /// The component's stable name.
        component: &'static str,
    },
    /// A query writes a shared component (one value per archetype).
    #[error(
        "{component} is a shared component and is read-only in queries; insert a new value to change it"
    )]
    SharedReadOnly {
        /// The component's stable name.
        component: &'static str,
    },
    /// A declared resource is not in the world.
    #[error("resource {resource} is not in the world; insert it before the simulation runs")]
    MissingResource {
        /// The resource's type name.
        resource: &'static str,
    },
    /// A presentation processor asked for a command buffer.
    #[error(
        "presentation processor {processor} cannot issue commands; structural changes belong in a simulation phase"
    )]
    CommandsInPresentation {
        /// The processor.
        processor: String,
    },
}

/// A schedule that cannot run (found when a [`crate::SimLoop`] is built).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScheduleError {
    /// Order constraints form a cycle.
    #[error(
        "processors {processors:?} order each other in a cycle; remove one before/after constraint"
    )]
    Cycle {
        /// The processors in the cycle.
        processors: Vec<String>,
    },
    /// A before/after constraint names no processor of the same phase.
    #[error(
        "processor {processor} is ordered against {target}, which is not a processor in its phase"
    )]
    UnknownProcessor {
        /// The constrained processor.
        processor: String,
        /// The missing name.
        target: String,
    },
    /// Two processors share a name.
    #[error("two processors are named {name}; names must be unique")]
    DuplicateName {
        /// The repeated name.
        name: String,
    },
    /// A presentation processor writes simulation state.
    #[error(
        "presentation processor {processor} writes {component}, which is simulation state; declare it with writes_presentation or move the processor to a simulation phase"
    )]
    PresentationWritesSimulation {
        /// The processor.
        processor: String,
        /// The component.
        component: String,
    },
    /// A simulation processor reads or writes presentation-only state.
    #[error(
        "simulation processor {processor} uses {component}, which is presentation-only and not hashed; simulation must not depend on it"
    )]
    SimulationReadsPresentation {
        /// The processor.
        processor: String,
        /// The component.
        component: String,
    },
}
