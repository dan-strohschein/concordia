//! Vendored Recast and Detour v1.6.0 (zlib) through Concordia's flat C shim, and the safe API
//! over it. This is the only crate in the workspace with `unsafe` for navigation (ADR-0077).

mod error;
mod ffi;
mod navmesh;
mod query;
mod tile;

pub use error::{BuildStage, RecastError};
pub use navmesh::{DtNavMesh, NavMeshParams, Poly, PolyRef, TileInfo};
pub use query::{AreaCosts, Corridor, DtQuery, RayResult};
pub use tile::{Area, BoxMark, BuildConfig, TileData, TileInput, build_tile};

/// The vendored recastnavigation release.
pub const RECAST_VERSION: &str = "1.6.0";

/// Most vertices a navmesh polygon has.
pub const MAX_VERTS_PER_POLY: usize = ffi::CR_MAX_VERTS_PER_POLY;

/// Recast/Detour heap blocks currently allocated (counted by the shim); used by leak tests.
pub fn live_allocations() -> i64 {
    // SAFETY: reads an atomic counter.
    unsafe { ffi::cr_live_allocations() }
}

/// Structs whose Rust size differs from the C compiler's (empty when the bindings match).
pub fn abi_mismatches() -> Vec<String> {
    let rust = [
        ("cr_config", size_of::<ffi::cr_config>()),
        ("cr_box_mark", size_of::<ffi::cr_box_mark>()),
        ("cr_tile_input", size_of::<ffi::cr_tile_input>()),
        ("cr_poly", size_of::<ffi::cr_poly>()),
        ("cr_filter", size_of::<ffi::cr_filter>()),
    ];
    rust.iter()
        .enumerate()
        .filter_map(|(i, (name, size))| {
            // SAFETY: a pure function of its argument.
            let c = unsafe { ffi::cr_struct_size(i as i32) };
            (c != *size as i32).then(|| format!("{name}: Rust {size} bytes, C {c}"))
        })
        .collect()
}
