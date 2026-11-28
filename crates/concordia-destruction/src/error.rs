//! Destruction errors.

/// Why a breakable couldn't be set up.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DestructionError {
    /// Its recipe or pieces couldn't be loaded.
    #[error("breakable {recipe}: {why}")]
    Load {
        /// The recipe's path.
        recipe: String,
        /// Why.
        why: String,
    },
}
