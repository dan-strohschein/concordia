//! Fracture authoring (M8 8.1, ADR-0180): a closed mesh cut into pieces by a seeded pattern of
//! convex cells, each clipped through Manifold (`manifold-sys`).

mod bounds;
mod cut;
mod error;
mod methods;
mod module;
mod pattern;
mod piece;
mod recipe;
mod store;
mod weld;

pub use bounds::Bounds;
pub use cut::{Fracture, Piece, Slot, fracture};
pub use error::FractureError;
pub use manifold_sys::safe::Mesh;
pub use methods::{
    FractureGenerate, FractureGenerateParams, FractureGenerateResult, FractureInspect,
    FractureInspectParams, FractureInspectResult, PieceSummary,
};
pub use module::FractureModule;
pub use pattern::{Cell, MAX_CELLS, Pattern};
pub use recipe::{Breakable, EXTENSION, FRACTURE_VERSION};
pub use store::{Stored, cache_key, pieces};
pub use weld::weld;
