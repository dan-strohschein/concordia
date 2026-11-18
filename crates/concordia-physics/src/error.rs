//! Physics errors.

/// A physics operation failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PhysicsError {
    /// Jolt could not create the world (bad settings or initialization failure).
    #[error("could not create the physics world: {0}")]
    World(String),
    /// A step failed (Jolt reported errors such as too many contacts).
    #[error("physics step failed with Jolt error bits {0:#x}; raise the body or contact limits")]
    Step(u32),
    /// Saved state could not be restored (different bodies, or corrupt data).
    #[error("physics state could not be restored; the body set must match the saved one")]
    Restore,
}
