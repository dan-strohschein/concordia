//! Math types (re-exported from `glam`, so every crate uses one version) and rays.
//!
//! ```
//! use concordia_core::math::{Quat, Vec3};
//! assert_eq!(Quat::IDENTITY * Vec3::X, Vec3::X);
//! ```

pub use glam::{
    Affine3A, EulerRot, IVec2, IVec3, Mat3, Mat4, Quat, UVec2, UVec3, Vec2, Vec3, Vec4,
};

/// View and projection constructors (Metal uses the `rh::proj::directx` convention: Z in [0, 1], Y-up).
pub use glam::camera;

/// Invalid geometric input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MathError {
    /// A direction had zero length.
    #[error("direction has zero length")]
    ZeroDirection,
    /// A value was NaN or infinite.
    #[error("value is NaN or infinite")]
    NonFinite,
}

/// A half-line with a unit direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    origin: Vec3,
    dir: Vec3,
}

impl Ray {
    /// A ray from `origin` along `dir` (normalized); rejects zero or non-finite input.
    pub fn new(origin: Vec3, dir: Vec3) -> Result<Self, MathError> {
        if !origin.is_finite() || !dir.is_finite() {
            return Err(MathError::NonFinite);
        }
        let dir = dir.try_normalize().ok_or(MathError::ZeroDirection)?;
        Ok(Self { origin, dir })
    }

    /// Start point.
    pub fn origin(&self) -> Vec3 {
        self.origin
    }

    /// Unit direction.
    pub fn dir(&self) -> Vec3 {
        self.dir
    }

    /// The point at distance `t` along the ray.
    pub fn at(&self, t: f32) -> Vec3 {
        self.origin + self.dir * t
    }

    /// Distance to the plane through `point` with `normal`, if the ray hits it in front of the origin.
    pub fn intersect_plane(&self, point: Vec3, normal: Vec3) -> Option<f32> {
        let denom = self.dir.dot(normal);
        if denom.abs() <= f32::EPSILON {
            return None;
        }
        let t = (point - self.origin).dot(normal) / denom;
        (t >= 0.0).then_some(t)
    }
}
