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
    /// A component's value is out of range.
    #[error("{0}")]
    Invalid(String),
    /// The points have no convex hull (fewer than 4, all in a plane, or not finite).
    #[error("these points have no convex hull: {0}")]
    Hull(String),
    /// Saved state could not be restored (different bodies, or corrupt data).
    #[error("physics state could not be restored; the body set must match the saved one")]
    Restore,
}
