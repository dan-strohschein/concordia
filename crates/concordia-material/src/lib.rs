//! Substrate-style material evaluation (ADR-0068): materials compiled into closure programs, the
//! per-pixel closure budget with its downgrade report, and the CPU reference BSDF the renderer's
//! shaders must match. The material model itself lives in `concordia-assets`.

mod bsdf;
mod budget;
pub mod dfg;
mod film;
mod program;
pub mod sampling;

pub use bsdf::{SSS_SCALE, eval_slab, slab_transmittance};
pub use budget::{BudgetError, ClosureBudget, ClosureMerge, ClosureReport, MergeKind, fit_budget};
pub use film::{WAVELENGTHS_NM, thin_film_reflectance};
pub use program::{MaterialProgram, Op, Shaded};

/// The folder holding `material.slang`, the GPU mirror of this crate's BSDF (add it to a shader's
/// include directories and `import material;`).
pub fn shader_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders"))
}
