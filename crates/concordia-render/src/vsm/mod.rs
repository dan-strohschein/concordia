//! Virtual shadow maps (M4 4.7, ADR-0091): page-based shadows for the sun (a clipmap) with a
//! physical page pool, page marking from scene depth and GPU page allocation; CSM is the fallback.

mod clipmap;
mod pool;
mod settings;
mod table;

pub use clipmap::{
    ClipLevel, Clipmap, LEVEL0_EXTENT, level_for_footprint, pixel_footprint, stable_depth_range,
    texel_world,
};
pub use pool::{PoolError, PoolLayout};
pub use settings::{ShadowTechnique, VsmSettings};
pub use table::{PAGE_TEXELS, PAGES_PER_SIDE, PageEntry, PageFlags, VIRTUAL_TEXELS, table_slot};
