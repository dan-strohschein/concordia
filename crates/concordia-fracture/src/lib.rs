//! Fracture authoring (M8 8.1, ADR-0180): a closed mesh cut into pieces by a seeded pattern of
//! convex cells, each clipped through Manifold (`manifold-sys`).

mod bounds;
mod cut;
mod error;
mod pattern;

pub use bounds::Bounds;
pub use cut::{Fracture, Piece, Slot, fracture};
pub use error::FractureError;
pub use manifold_sys::safe::Mesh;
pub use pattern::{Cell, MAX_CELLS, Pattern};
