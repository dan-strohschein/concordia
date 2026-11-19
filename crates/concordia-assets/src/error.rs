//! Asset errors.

use std::path::PathBuf;

/// An asset operation failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AssetError {
    /// A file could not be read or written.
    #[error("asset io: {0}")]
    Io(String),
    /// A path is absolute or leaves the project.
    #[error("asset path {0:?} must be relative and inside the project")]
    Path(String),
    /// No importer handles the file's extension.
    #[error("no importer for {path} (extension {extension:?}); known: {known}")]
    NoImporter {
        /// The file.
        path: PathBuf,
        /// Its extension.
        extension: String,
        /// Extensions with importers.
        known: String,
    },
    /// An importer rejected the source.
    #[error("importing {path}: {reason}")]
    Import {
        /// The file.
        path: PathBuf,
        /// Why.
        reason: String,
    },
    /// A sidecar is malformed.
    #[error("sidecar {path}: {reason}")]
    Sidecar {
        /// The sidecar.
        path: PathBuf,
        /// Why.
        reason: String,
    },
    /// A reference names nothing known.
    #[error("unknown asset {0}")]
    NotFound(String),
    /// A reference is malformed.
    #[error("invalid asset reference {input:?}: {reason}")]
    Reference {
        /// The text.
        input: String,
        /// Why.
        reason: String,
    },
    /// Derived data could not be decoded (the DDC entry is rebuilt on the next load).
    #[error("derived data: {0}")]
    Decode(String),
}

pub(crate) fn io(path: &std::path::Path, e: impl std::fmt::Display) -> AssetError {
    AssetError::Io(format!("{}: {e}", path.display()))
}
