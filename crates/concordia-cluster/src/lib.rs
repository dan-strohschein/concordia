//! Meshlet building for GPU culling (M4 4.2, ADR-0100): splits a static mesh into small
//! clusters with bounding spheres and normal cones, deterministically for any thread count.
//! All `unsafe` lives in `meshopt-sys`.

#![forbid(unsafe_code)]

mod build;
mod error;
mod input;
mod meshlet;
mod settings;

pub use build::build;
pub use error::ClusterError;
pub use input::ClusterInput;
pub use meshlet::{Meshlet, MeshletMesh, NormalCone, Sphere};
pub use settings::{MeshletBuildSettings, MeshletSize};
