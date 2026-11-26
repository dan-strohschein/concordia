//! The physical page pool: `u32` depth texels in bindless buffers of at most 1024 pages (64 MB)
//! each, so no buffer nears a device's storage-range limit.

use super::table::PAGE_TEXELS;

/// Pages per pool buffer.
pub const CHUNK_PAGES: u32 = 1024;
/// `u32` texels per page.
const PAGE_WORDS: u32 = PAGE_TEXELS * PAGE_TEXELS;

/// A pool size outside 64..=16 384 pages.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("pool_pages must be 64..=16384, not {0}")]
pub struct PoolError(pub u32);

/// Where each physical page lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolLayout {
    pages: u32,
}

impl PoolLayout {
    /// A pool of `pages` (64..=16 384).
    pub fn new(pages: u32) -> Result<Self, PoolError> {
        match (64..=16_384).contains(&pages) {
            true => Ok(Self { pages }),
            false => Err(PoolError(pages)),
        }
    }

    /// Physical pages.
    pub fn pages(&self) -> u32 {
        self.pages
    }

    /// Buffers.
    pub fn chunks(&self) -> u32 {
        self.pages.div_ceil(CHUNK_PAGES)
    }

    /// Bytes of buffer `chunk` (0 past the last).
    pub fn chunk_bytes(&self, chunk: u32) -> u64 {
        let pages = self
            .pages
            .saturating_sub(chunk * CHUNK_PAGES)
            .min(CHUNK_PAGES);
        u64::from(pages) * u64::from(PAGE_WORDS) * 4
    }

    /// Bytes of the whole pool.
    pub fn total_bytes(&self) -> u64 {
        u64::from(self.pages) * u64::from(PAGE_WORDS) * 4
    }

    /// Buffer and first `u32` of physical page `page`.
    pub fn address(&self, page: u32) -> (u32, u32) {
        debug_assert!(page < self.pages, "page {page} of {}", self.pages);
        (page / CHUNK_PAGES, page % CHUNK_PAGES * PAGE_WORDS)
    }
}
