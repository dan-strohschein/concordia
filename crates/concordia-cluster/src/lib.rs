//! Meshlet building for GPU culling (M4 4.2, ADR-0100): splits a static mesh into small
//! clusters with bounding spheres and normal cones, deterministically for any thread count.
//! All `unsafe` lives in `meshopt-sys`.

#![forbid(unsafe_code)]

mod blob;
mod build;
mod error;
mod input;
mod meshlet;
mod settings;
mod stats;

pub use blob::{FORMAT_VERSION, cache_key};
pub use build::{PILE_TRIANGLES, Pile, build, piles};
pub use error::ClusterError;
pub use input::ClusterInput;
pub use meshlet::{Meshlet, MeshletMesh, NormalCone, Sphere};
pub use settings::{AUTO_MIN_TRIANGLES, MeshletBuildSettings, MeshletSize};
pub use stats::{MeshletStats, Spread};
