//! Scene errors: positions are 1-based line and column (in characters).

use concordia_core::id::{EntityId, IdError};
use concordia_reflect::ReflectError;
use concordia_terra::EcsError;

/// A scene or prefab could not be read, written or loaded.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SceneError {
    /// The text is not valid scene RON.
    #[error("line {line}, col {col}: {message}")]
    Parse {
        /// Line.
        line: usize,
        /// Column.
        col: usize,
        /// What is wrong.
        message: String,
    },
    /// A component name is not a registered component.
    #[error(
        "line {line}, col {col}: unknown component {name:?}{}",
        hint(did_you_mean)
    )]
    UnknownComponent {
        /// The name in the file.
        name: String,
        /// Line.
        line: usize,
        /// Column.
        col: usize,
        /// The closest registered component name.
        did_you_mean: Option<String>,
    },
    /// A component value has the wrong shape or is out of range.
    #[error(
        "line {line}, col {col}: invalid {component} at {pointer:?}: expected {expected}, found {found}"
    )]
    InvalidValue {
        /// Component name.
        component: String,
        /// JSON Pointer inside the component.
        pointer: String,
        /// What the type expects.
        expected: String,
        /// What the file has.
        found: String,
        /// Line.
        line: usize,
        /// Column.
        col: usize,
    },
    /// Two entities share an id.
    #[error("line {line}, col {col}: duplicate entity id {id}")]
    DuplicateEntity {
        /// The repeated id.
        id: EntityId,
        /// Line.
        line: usize,
        /// Column.
        col: usize,
    },
    /// The file was written by a newer engine.
    #[error(
        "scene format {found} is newer than this engine supports ({supported}); upgrade Concordia to open it"
    )]
    UnsupportedFormat {
        /// Version in the file.
        found: u64,
        /// Highest supported version.
        supported: u32,
    },
    /// A value cannot be saved because it contains NaN or infinity.
    #[error("cannot save {component} on {entity}: non-finite number at {pointer:?}")]
    NonFinite {
        /// The entity.
        entity: EntityId,
        /// Component name.
        component: String,
        /// JSON Pointer inside the component.
        pointer: String,
    },
    /// A world operation failed while loading.
    #[error(transparent)]
    World(#[from] EcsError),
    /// A value could not be converted.
    #[error(transparent)]
    Reflect(#[from] ReflectError),
    /// Id generation failed.
    #[error(transparent)]
    Id(#[from] IdError),
}

fn hint(did_you_mean: &Option<String>) -> String {
    did_you_mean
        .as_ref()
        .map(|s| format!("; did you mean {s:?}?"))
        .unwrap_or_default()
}
