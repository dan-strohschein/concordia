//! Bounding-volume hierarchies for ray queries: a binned-SAH tree over boxes, triangle meshes over
//! it, and a two-level structure of instanced meshes (M4 4.8; moved from `concordia-pathtrace`).

pub mod accel;
pub mod bvh;
mod error;
pub mod geometry;

pub use accel::{Accel, Instance, InstanceHit};
pub use bvh::{Bvh, Node, Tree};
pub use error::BvhError;
pub use geometry::{Hit, Ray, Triangle};
