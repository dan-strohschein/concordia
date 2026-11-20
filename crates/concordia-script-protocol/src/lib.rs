//! The script protocol (ADR-0062): what a gameplay module declares and what it exchanges with
//! the engine each tick. Backends (WASM, native, in-process) are bindings of these types; the
//! WIT world `concordia:game/gameplay` is the WASM one.

use concordia_core::id::EntityId;
use concordia_reflect::TypeDesc;
use serde::{Deserialize, Serialize};

pub mod codec;
mod ids;
mod validate;

pub use ids::{ID_BYTES, ids_from_bytes, ids_to_bytes};

/// The protocol's version; a module built against another major is refused.
pub const PROTOCOL_VERSION: u32 = 1;

/// A protocol violation: a malformed description or run output.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    /// A description is malformed.
    #[error("invalid module description: {0}")]
    InvalidDesc(String),
    /// Packed entity ids are malformed.
    #[error("invalid entity ids: {0}")]
    InvalidIds(String),
    /// A native run envelope is malformed.
    #[error("invalid run envelope: {0}")]
    InvalidEnvelope(String),
}

/// A processing phase, in tick order (mirrors Terra's `Phase`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum Phase {
    /// Input and intent.
    PrePhysics,
    /// The physics step.
    Physics,
    /// Gameplay reacting to physics (the default).
    #[default]
    PostPhysics,
    /// End-of-tick bookkeeping.
    FrameEnd,
    /// Presentation-only state.
    Presentation,
}

/// A processor: phase, access by stable name, and order constraints.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProcessorDesc {
    /// Unique name (Terra's processor name).
    pub name: String,
    /// The phase it runs in.
    pub phase: Phase,
    /// Components read; every batch row has them.
    pub reads: Vec<String>,
    /// Components written; every batch row has them.
    pub writes: Vec<String>,
    /// Components a row must also have (not delivered).
    pub with: Vec<String>,
    /// Components a row must not have.
    pub without: Vec<String>,
    /// Signals delivered to it.
    pub receives: Vec<String>,
    /// Processors that run before it (same phase).
    pub after: Vec<String>,
    /// Processors that run after it (same phase).
    pub before: Vec<String>,
}

/// A trait: components (with their type defaults) and the processors its entities need.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TraitDesc {
    /// Stable name, used in `.template.ron` files.
    pub name: String,
    /// Doc text.
    pub docs: String,
    /// Component names it adds.
    pub components: Vec<String>,
    /// Processors its entities need.
    pub requires: Vec<String>,
}

/// Everything a module declares.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ModuleDesc {
    /// Module name (the `script:<name>` actor).
    pub name: String,
    /// Module version (informational; reloads compare types, not this).
    pub version: u32,
    /// Helper types, registered first, in order.
    pub types: Vec<TypeDesc>,
    /// Component types.
    pub components: Vec<TypeDesc>,
    /// Signal types.
    pub signals: Vec<TypeDesc>,
    /// Traits.
    pub traits: Vec<TraitDesc>,
    /// Processors.
    pub processors: Vec<ProcessorDesc>,
}

/// Simulation time and the processor's seed for one run.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct TickContext {
    /// The tick being simulated.
    pub tick: u64,
    /// Simulated seconds this tick.
    pub dt: f32,
    /// Simulated seconds since play started.
    pub time: f64,
    /// Derived from the session seed, the tick and the processor's name.
    pub seed: u64,
}

/// One component's values for every row, as a wire column.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Column {
    /// Component name.
    pub name: String,
    /// Wire bytes (`rows × stride` then the arena).
    pub data: Vec<u8>,
}

/// Delivered signals of one type, by entity then raise order.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SignalColumn {
    /// Signal type name.
    pub name: String,
    /// Target of each row.
    pub targets: Vec<EntityId>,
    /// Payloads as a wire column.
    pub data: Vec<u8>,
}

/// A processor's matches: ids, declared columns (reads then writes) and delivered signals.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Batch {
    /// Matching entities; row `i` is `ids[i]`.
    pub ids: Vec<EntityId>,
    /// One column per declared read or write.
    pub columns: Vec<Column>,
    /// One column per received signal type.
    pub signals: Vec<SignalColumn>,
}

impl Batch {
    /// The column of component `name`.
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// The delivered signals of type `name`.
    pub fn signal(&self, name: &str) -> Option<&SignalColumn> {
        self.signals.iter().find(|c| c.name == name)
    }
}

/// A command's target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    /// An existing entity.
    Entity(EntityId),
    /// The n-th spawn (`Spawn` or `SpawnTemplate`) of this run.
    Pending(u32),
}

impl From<EntityId> for Target {
    fn from(id: EntityId) -> Self {
        Self::Entity(id)
    }
}

/// One component or signal value: a one-row wire column.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Value {
    /// Type name.
    pub name: String,
    /// One-row wire bytes.
    pub data: Vec<u8>,
}

/// A structural change, applied at the end of the phase in plan order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Spawns an entity with these components.
    Spawn(Vec<Value>),
    /// Spawns an entity from the named template.
    SpawnTemplate(String),
    /// Inserts or replaces a component.
    Insert(Target, Value),
    /// Removes the named component.
    Remove(Target, String),
    /// Despawns an entity.
    Despawn(Target),
    /// Raises a signal (delivered next tick).
    Signal(Target, Value),
}

/// A run's effects: full columns written back to the batch rows, then commands.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RunOutput {
    /// Written columns (each a declared write, one row per batch id).
    pub writes: Vec<Column>,
    /// Commands, in recording order.
    pub commands: Vec<Command>,
}
