//! Math types (re-exported from `glam`, so every crate uses one version).
//!
//! ```
//! use concordia_core::math::{Quat, Vec3};
//! assert_eq!(Quat::IDENTITY * Vec3::X, Vec3::X);
//! ```

pub use glam::{Affine3A, Mat4, Quat, Vec2, Vec3, Vec4};
