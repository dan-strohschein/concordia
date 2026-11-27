//! Errors from updating a hierarchy.

/// What went wrong updating a hierarchy in place.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BvhError {
    /// A refit got a different number of primitives than the tree was built over.
    #[error("refit with {got} primitives, built over {expected}")]
    CountMismatch {
        /// Primitives the tree holds.
        expected: usize,
        /// Primitives given.
        got: usize,
    },
    /// A transform names an instance that doesn't exist.
    #[error("instance {index} out of {count}")]
    NoInstance {
        /// The named instance.
        index: u32,
        /// Instances present.
        count: usize,
    },
    /// A transform can't be inverted (rays couldn't be moved into the instance).
    #[error("instance {index}'s transform is singular")]
    Singular {
        /// The instance.
        index: u32,
    },
}
