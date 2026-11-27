//! `render.stats.shadows`: which shadow technique ran, why `vsm` fell back, and the virtual shadow
//! maps' page counts (read back when the frame's GPU work is done).

use concordia_reflect::Reflect;
use concordia_rhi::{Buffer, Device};
use serde::{Deserialize, Serialize};

use super::gpu::COUNT_WORDS;
use super::settings::ShadowTechnique;
use crate::RenderError;

/// The sun's shadows in the last frame.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct ShadowStats {
    /// The technique asked for (`[render] shadows` or `render.set_shadows`).
    pub requested: ShadowTechnique,
    /// The technique that ran (`csm` when `vsm` can't run on the device).
    pub technique: ShadowTechnique,
    /// Why `vsm` fell back to `csm`, when it did.
    pub reason: Option<String>,
    /// Physical pages in the pool (virtual shadow maps).
    pub pool_pages: u32,
    /// Clipmap levels.
    pub levels: u32,
    /// Pages some pixel asked for.
    pub pages_requested: u64,
    /// Requested pages that got a physical page (and were drawn).
    pub pages_allocated: u64,
    /// Requested pages the pool had no room for (their pixels read a coarser level).
    pub pages_starved: u64,
    /// Meshlets drawn into the levels (each culled for its level and its pages).
    pub shadow_meshlets: u64,
    /// Caster-page pairs drawn: each caster drawn once per page due under it (4.7a2).
    pub page_draws: u64,
    /// Pairs past the pair buffers' room, not drawn (their pages lose those casters' shadows).
    pub page_draws_dropped: u64,
}

/// A frame's VSM counts and the levels' cull codes on their way back (readback buffers).
#[derive(Debug, Clone)]
pub(crate) struct VsmReadback {
    pub(crate) counts: Buffer,
    /// The levels' cull codes and how many there are.
    pub(crate) codes: Option<(Buffer, u32)>,
    /// Each binning's (pairs wanted, pairs kept).
    pub(crate) totals: Vec<Buffer>,
}

impl VsmReadback {
    /// Reads the counts into `stats`.
    pub(crate) fn read(
        &self,
        device: &dyn Device,
        stats: &mut ShadowStats,
    ) -> Result<(), RenderError> {
        let words = words_of(device, self.counts, COUNT_WORDS)?;
        stats.pages_requested = u64::from(words[0]);
        stats.pages_allocated = u64::from(words[1]);
        stats.pages_starved = u64::from(words[2]);
        stats.shadow_meshlets = match self.codes {
            Some((buffer, count)) if count > 0 => words_of(device, buffer, count)?
                .iter()
                .filter(|w| crate::cull_stats::code_of(**w) == Some(crate::CullCode::Drawn))
                .count() as u64,
            _ => 0,
        };
        let (mut wanted, mut kept) = (0u64, 0u64);
        for buffer in &self.totals {
            let w = words_of(device, *buffer, 2)?;
            (wanted, kept) = (wanted + u64::from(w[0]), kept + u64::from(w[1]));
        }
        (stats.page_draws, stats.page_draws_dropped) = (kept, wanted - kept);
        Ok(())
    }
}

fn words_of(device: &dyn Device, buffer: Buffer, count: u32) -> Result<Vec<u32>, RenderError> {
    Ok(device
        .read_buffer(buffer, 0, u64::from(count) * 4)?
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect())
}
