//! Virtual shadow maps (M4 4.7, ADR-0091): page-based shadows for the sun (a clipmap) with a
//! physical page pool, page marking from scene depth and GPU page allocation; CSM is the fallback.

mod api;
mod bin;
mod cache;
mod clipmap;
mod frame;
mod gpu;
mod job;
mod ledger;
pub mod local;
mod pool;
mod settings;
mod stats;
mod table;
#[cfg(test)]
pub(crate) mod testing;

pub use api::{RenderSetShadows, RenderSetShadowsParams, RenderSetShadowsResult};
pub use bin::caster_sphere;
pub(crate) use bin::{
    BinInputs, BinPipelines, DrawSpan, MeshletBins, Unit, UnitBins, units_for, world_sphere,
};
pub use cache::{CacheKey, CacheReset, CacheState};
pub use clipmap::{
    ClipLevel, Clipmap, LEVEL0_EXTENT, level_for_footprint, pixel_footprint, stable_depth_range,
    texel_world,
};
pub use frame::{Footprint, floor_level};
pub(crate) use gpu::{COUNT_WORDS, VsmPipelines};
pub(crate) use job::{
    CasterDraws, CasterLayer, JobIn, VsmJob, VsmResources, add_page_passes, add_render_pass,
};
pub use ledger::{CasterInput, CasterLedger, CasterState, DYNAMIC_FRAMES, LedgerFrame, caster_box};
pub use pool::{PoolError, PoolLayout};
pub use settings::{ShadowTechnique, Shadows, VsmSettings};
pub use stats::ShadowStats;
pub(crate) use stats::VsmReadback;
pub use table::{
    PAGE_TEXELS, PAGES_PER_SIDE, PageEntry, PageFlags, VIRTUAL_TEXELS, slot_page, table_slot,
};

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
