//! Virtual shadow maps (M4 4.7, ADR-0091): page-based shadows for the sun (a clipmap) with a
//! physical page pool, page marking from scene depth and GPU page allocation; CSM is the fallback.

mod clipmap;
mod frame;
mod gpu;
mod job;
mod pool;
mod settings;
mod stats;
mod table;

pub use clipmap::{
    ClipLevel, Clipmap, LEVEL0_EXTENT, level_for_footprint, pixel_footprint, stable_depth_range,
    texel_world,
};
pub use frame::{Footprint, floor_level};
pub(crate) use gpu::{COUNT_WORDS, VsmPipelines, VsmPool};
pub(crate) use job::{CasterDraws, JobIn, VsmJob, add_page_passes, add_render_pass};
pub use pool::{PoolError, PoolLayout};
pub use settings::{ShadowTechnique, VsmSettings};
pub use stats::ShadowStats;
pub(crate) use stats::VsmReadback;
pub use table::{PAGE_TEXELS, PAGES_PER_SIDE, PageEntry, PageFlags, VIRTUAL_TEXELS, table_slot};

use concordia_rhi::RhiCaps;

/// Whether a device can run virtual shadow maps, or why not (the renderer then runs CSM).
pub fn supported(caps: &RhiCaps) -> Result<(), &'static str> {
    if !caps.bindless {
        return Err("no bindless resources");
    }
    if !caps.fragment_atomics {
        return Err("no fragment-shader atomics");
    }
    if caps.max_render_extent < VIRTUAL_TEXELS {
        return Err("attachment-less passes smaller than 16384 pixels");
    }
    Ok(())
}
