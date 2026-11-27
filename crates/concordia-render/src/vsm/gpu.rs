//! The GPU side of virtual shadow maps' pages (M4 4.7a, 4.7b): the physical pool, the persistent
//! page store, and one frame's page work (`vsm_page.slang`): age the kept pages (releasing slots
//! scrolled out), mark from the camera's depth, decide residency coarse levels first, release the
//! starved and the old, evict the oldest when new pages need room, allocate from the free list,
//! list the pages to draw and clear them.

use std::num::NonZeroU64;

use concordia_rhi::{
    Access, AccessFlags, Buffer, BufferBarrier, BufferDesc, BufferUsage, Command, Device, Layout,
    MemoryLocation, Pipeline, Stages,
};

use super::pool::PoolLayout;
use super::table::{PAGES_PER_SIDE, PageFlags};
use crate::RenderError;
use crate::gpu::Uploads;
use crate::scan::{Scan, ScanPipelines, compute_barrier};

/// Table entries per level.
const LEVEL_ENTRIES: u32 = PAGES_PER_SIDE * PAGES_PER_SIDE;
/// Words of the eviction histogram (one per age) and its result (threshold, quota, deficit).
const HISTOGRAM_WORDS: u32 = 256 + 4;
/// Words of the counts buffer: requested, resident, starved, the clear's dispatch (3 words), new,
/// drawn, evicted, kept, pool used, the composite's dispatch (3 words), dynamic pages, dynamic
/// pages without room (drawn into the static page).
pub(crate) const COUNT_WORDS: u32 = 16;
/// Marks a physical page's owner word as a dynamic page (freed at the next frame's start; mirrored
/// by `VSM_DYNAMIC_OWNER`).
#[cfg(test)]
pub(crate) const DYNAMIC_OWNER: u32 = 1 << 31;

/// The page work's compute pipelines.
#[derive(Debug, Clone, Copy)]
pub(crate) struct VsmPipelines {
    pub(crate) zero: Pipeline,
    pub(crate) age: Pipeline,
    pub(crate) invalidate: Pipeline,
    pub(crate) release_dynamic: Pipeline,
    pub(crate) entry_flags: Pipeline,
    pub(crate) dynamic_allocate: Pipeline,
    pub(crate) composite: Pipeline,
    pub(crate) mark: Pipeline,
    pub(crate) flags: Pipeline,
    pub(crate) update: Pipeline,
    pub(crate) free_flags: Pipeline,
    pub(crate) age_histogram: Pipeline,
    pub(crate) threshold: Pipeline,
    pub(crate) age_flags: Pipeline,
    pub(crate) evict: Pipeline,
    pub(crate) free_list: Pipeline,
    pub(crate) allocate: Pipeline,
    pub(crate) draw_list: Pipeline,
    pub(crate) counts: Pipeline,
    pub(crate) clear: Pipeline,
    pub(crate) scan: ScanPipelines,
}

impl VsmPipelines {
    /// Every page-work kernel of `vsm_page.slang`, made by `kernel`, with the shared scan.
    pub(crate) fn create(
        mut kernel: impl FnMut(&str) -> Result<Pipeline, RenderError>,
        scan: ScanPipelines,
    ) -> Result<Self, RenderError> {
        Ok(Self {
            zero: kernel("vsm_zero")?,
            age: kernel("vsm_age")?,
            invalidate: kernel("vsm_flag_spheres")?,
            release_dynamic: kernel("vsm_release_dynamic")?,
            entry_flags: kernel("vsm_entry_flags")?,
            dynamic_allocate: kernel("vsm_dynamic_allocate")?,
            composite: kernel("vsm_composite")?,
            mark: kernel("vsm_mark")?,
            flags: kernel("vsm_request_flags")?,
            update: kernel("vsm_update")?,
            free_flags: kernel("vsm_free_flags")?,
            age_histogram: kernel("vsm_age_histogram")?,
            threshold: kernel("vsm_evict_threshold")?,
            age_flags: kernel("vsm_age_flags")?,
            evict: kernel("vsm_evict")?,
            free_list: kernel("vsm_free_list")?,
            allocate: kernel("vsm_allocate")?,
            draw_list: kernel("vsm_draw_list")?,
            counts: kernel("vsm_counts")?,
            clear: kernel("vsm_clear")?,
            scan,
        })
    }
}

/// The physical page pool: persistent buffers of at most 1024 pages, re-made when its size changes.
#[derive(Default)]
pub(crate) struct VsmPool {
    layout: Option<PoolLayout>,
    chunks: Vec<(Buffer, u32)>,
}

impl VsmPool {
    /// The pool's buffers (and bindless indices) for `layout`.
    pub(crate) fn ensure(
        &mut self,
        device: &dyn Device,
        layout: PoolLayout,
    ) -> Result<&[(Buffer, u32)], RenderError> {
        if self.layout != Some(layout) {
            self.destroy(device);
            for chunk in 0..layout.chunks() {
                let buffer = device.create_buffer(&BufferDesc {
                    size: NonZeroU64::new(layout.chunk_bytes(chunk)).unwrap_or(NonZeroU64::MIN),
                    usage: BufferUsage::STORAGE
                        | BufferUsage::BINDLESS
                        | BufferUsage::COPY_SRC
                        | BufferUsage::COPY_DST,
                    memory: MemoryLocation::GpuOnly,
                    label: format!("vsm pool {chunk}"),
                })?;
                self.chunks.push((buffer, device.buffer_index(buffer)?.0));
            }
            self.layout = Some(layout);
        }
        Ok(&self.chunks)
    }

    /// Frees the buffers.
    pub(crate) fn destroy(&mut self, device: &dyn Device) {
        self.chunks
            .drain(..)
            .for_each(|(b, _)| device.destroy_buffer(b));
        self.layout = None;
    }
}

/// The pages kept across frames (M4 4.7b): the page table and the physical pages' owners (entry
/// + 1, 0 free), re-made (and so cleared) when the levels or the pool change.
#[derive(Default)]
pub(crate) struct PageStore {
    shape: Option<(u32, u32)>,
    buffers: Option<(Buffer, Buffer)>,
}

impl PageStore {
    /// The table and owner buffers for `levels` levels over `pool` pages, and whether they are new
    /// (their contents undefined until cleared).
    pub(crate) fn ensure(
        &mut self,
        device: &dyn Device,
        (levels, pool): (u32, u32),
    ) -> Result<((Buffer, Buffer), bool), RenderError> {
        if let (Some(b), true) = (self.buffers, self.shape == Some((levels, pool))) {
            return Ok((b, false));
        }
        self.destroy(device);
        let make = |words: u32, label: &str| {
            device.create_buffer(&BufferDesc {
                size: NonZeroU64::new(u64::from(words) * 4).unwrap_or(NonZeroU64::MIN),
                usage: page_usage(),
                memory: MemoryLocation::GpuOnly,
                label: label.into(),
            })
        };
        let table = make(levels * LEVEL_ENTRIES, "vsm table")?;
        let pages = make(pool, "vsm page owners")?;
        self.buffers = Some((table, pages));
        self.shape = Some((levels, pool));
        Ok(((table, pages), true))
    }

    /// Frees the buffers.
    pub(crate) fn destroy(&mut self, device: &dyn Device) {
        if let Some((t, p)) = self.buffers.take() {
            device.destroy_buffer(t);
            device.destroy_buffer(p);
        }
        self.shape = None;
    }
}

fn page_usage() -> BufferUsage {
    BufferUsage::STORAGE
        | BufferUsage::BINDLESS
        | BufferUsage::COPY_SRC
        | BufferUsage::COPY_DST
        | BufferUsage::INDIRECT
}

/// One frame's page buffers and their recorded work.
#[derive(Debug, Clone)]
pub(crate) struct PageWork {
    pipelines: VsmPipelines,
    levels: u32,
    pool: u32,
    keep_frames: u32,
    /// The page table (levels × 128² entries; persistent).
    pub(crate) table: Buffer,
    /// Physical page → its table entry + 1 (0 free; persistent).
    pub(crate) pages: Buffer,
    /// [`COUNT_WORDS`] words.
    pub(crate) counts: Buffer,
    /// The physical pages to draw this frame, in allocation order.
    pub(crate) list: Buffer,
    /// Each entry's dynamic physical page + 1 this frame (0: none).
    pub(crate) dynamic: Buffer,
    /// (static page, dynamic page) per dynamic page, for the composite.
    pub(crate) copies: Buffer,
    bufs: Bufs,
    scans: Scans,
    index: Indices,
}

#[derive(Debug, Clone, Copy)]
struct Bufs {
    requests: Buffer,
    needs: Buffer,
    free: Buffer,
    histogram: Buffer,
    eq: Buffer,
    dirty: Buffer,
    free_list: Buffer,
    movers: Buffer,
}

#[derive(Debug, Clone)]
struct Scans {
    requests: Scan,
    needs: Scan,
    free: Scan,
    eq: Scan,
    dirty: Scan,
    movers: Scan,
}

#[derive(Debug, Clone, Copy)]
struct Indices {
    table: u32,
    pages: u32,
    counts: u32,
    list: u32,
    requests: u32,
    scanned: u32,
    needs: u32,
    need_scan: u32,
    free: u32,
    free_scan: u32,
    free_list: u32,
    histogram: u32,
    eq: u32,
    eq_scan: u32,
    dirty: u32,
    dirty_scan: u32,
    dynamic: u32,
    copies: u32,
    movers: u32,
    mover_scan: u32,
}

fn storage(
    device: &dyn Device,
    up: &Uploads,
    words: u32,
    label: &str,
) -> Result<Buffer, RenderError> {
    Ok(up.alloc(
        device,
        &BufferDesc {
            size: NonZeroU64::new(u64::from(words) * 4).unwrap_or(NonZeroU64::MIN),
            usage: page_usage(),
            memory: MemoryLocation::GpuOnly,
            label: label.into(),
        },
    )?)
}

fn words<const N: usize>(w: [u32; N]) -> Command {
    Command::PushConstants(w.iter().flat_map(|w| w.to_le_bytes()).collect())
}

/// A dispatch of 256-thread groups covering `count` threads.
fn dispatch(count: u32) -> Command {
    Command::Dispatch {
        groups: crate::scan::groups(count),
    }
}

/// What a frame's page work keeps and sizes: the persistent table and owners, the levels, the pool
/// and `keep_frames`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WorkShape {
    pub(crate) store: (Buffer, Buffer),
    pub(crate) levels: u32,
    pub(crate) pool: u32,
    pub(crate) keep_frames: u32,
}

impl PageWork {
    /// Allocates a frame's request, scan, free, eviction, list and count buffers (returned in
    /// `keep`) around the persistent `shape.store`.
    pub(crate) fn prepare(
        device: &dyn Device,
        up: &Uploads,
        pipelines: VsmPipelines,
        shape: WorkShape,
        keep: &mut Vec<Buffer>,
    ) -> Result<Self, RenderError> {
        let (levels, pool) = (shape.levels, shape.pool);
        let entries = levels * LEVEL_ENTRIES;
        let mut make = |words: u32, label: &str| -> Result<Buffer, RenderError> {
            let b = storage(device, up, words, label)?;
            keep.push(b);
            Ok(b)
        };
        let requests = make(entries, "vsm requests")?;
        let scanned = make(entries + 1, "vsm request scan")?;
        let needs = make(entries, "vsm needs")?;
        let need_scan = make(entries + 1, "vsm need scan")?;
        let free = make(pool, "vsm free")?;
        let free_scan = make(pool + 1, "vsm free scan")?;
        let free_list = make(pool, "vsm free list")?;
        let histogram = make(HISTOGRAM_WORDS, "vsm age histogram")?;
        let eq = make(pool, "vsm evict ties")?;
        let eq_scan = make(pool + 1, "vsm evict tie scan")?;
        let dirty = make(entries, "vsm dirty")?;
        let dirty_scan = make(entries + 1, "vsm dirty scan")?;
        let list = make(pool, "vsm draw list")?;
        let dynamic = make(entries, "vsm dynamic table")?;
        let copies = make(pool * 2, "vsm composite copies")?;
        let movers = make(entries, "vsm dynamic flags")?;
        let mover_scan = make(entries + 1, "vsm dynamic scan")?;
        let counts = make(COUNT_WORDS, "vsm counts")?;
        let scan = |(i, o), n, keep: &mut Vec<Buffer>| {
            Scan::prepare(device, up, pipelines.scan, (i, o), n, keep)
        };
        let scans = Scans {
            requests: scan((requests, scanned), entries, keep)?,
            needs: scan((needs, need_scan), entries, keep)?,
            free: scan((free, free_scan), pool, keep)?,
            eq: scan((eq, eq_scan), pool, keep)?,
            dirty: scan((dirty, dirty_scan), entries, keep)?,
            movers: scan((movers, mover_scan), entries, keep)?,
        };
        let index = |b: Buffer| device.buffer_index(b).map(|i| i.0);
        let (table, pages) = shape.store;
        let index = Indices {
            table: index(table)?,
            pages: index(pages)?,
            counts: index(counts)?,
            list: index(list)?,
            requests: index(requests)?,
            scanned: index(scanned)?,
            needs: index(needs)?,
            need_scan: index(need_scan)?,
            free: index(free)?,
            free_scan: index(free_scan)?,
            free_list: index(free_list)?,
            histogram: index(histogram)?,
            eq: index(eq)?,
            eq_scan: index(eq_scan)?,
            dirty: index(dirty)?,
            dirty_scan: index(dirty_scan)?,
            dynamic: index(dynamic)?,
            copies: index(copies)?,
            movers: index(movers)?,
            mover_scan: index(mover_scan)?,
        };
        Ok(Self {
            pipelines,
            levels,
            pool,
            keep_frames: shape.keep_frames,
            table,
            pages,
            counts,
            list,
            dynamic,
            copies,
            bufs: Bufs {
                requests,
                needs,
                free,
                histogram,
                eq,
                dirty,
                free_list,
                movers,
            },
            scans,
            index,
        })
    }

    fn entries(&self) -> u32 {
        self.levels * LEVEL_ENTRIES
    }

    /// Starts the frame: clears the store when `reset`, then ages the kept pages and releases slots
    /// scrolled out of their level (origins in parameter block `params`).
    pub(crate) fn begin(&self, reset: bool, params: u32) -> Vec<Command> {
        let (p, i) = (&self.pipelines, &self.index);
        let mut out = Vec::new();
        if reset {
            out.extend([
                Command::SetPipeline(p.zero),
                words([i.table, self.entries()]),
                dispatch(self.entries()),
                words([i.pages, self.pool]),
                dispatch(self.pool),
                compute_barrier(&[self.table, self.pages]),
            ]);
        }
        out.extend([
            Command::SetPipeline(p.release_dynamic),
            words([i.pages, self.pool]),
            dispatch(self.pool),
            compute_barrier(&[self.pages]),
            Command::SetPipeline(p.age),
            words([i.table, i.pages, params, self.levels]),
            dispatch(self.entries()),
            compute_barrier(&[self.table, self.pages]),
        ]);
        out
    }

    /// Flags the held pages under each of `count` world spheres (bindless buffer `spheres`, one
    /// `float4` each) in every level as invalidated (parameter block `params`, depth views `views`).
    pub(crate) fn invalidate(&self, spheres: (u32, u32), blocks: (u32, u32)) -> Vec<Command> {
        self.flag_spheres(spheres, blocks, (FLAG_INVALIDATED, FLAG_HELD))
    }

    /// Ors `flag` into every entry under each of `count` spheres that holds a page and has `need`.
    fn flag_spheres(
        &self,
        (spheres, count): (u32, u32),
        (params, views): (u32, u32),
        (flag, need): (u32, u32),
    ) -> Vec<Command> {
        if count == 0 {
            return Vec::new();
        }
        let rows = count.div_ceil(65_535);
        vec![
            Command::SetPipeline(self.pipelines.invalidate),
            words([spheres, count, params, views, self.index.table, flag, need]),
            Command::Dispatch {
                groups: [count.min(65_535), rows, self.levels],
            },
            compute_barrier(&[self.table]),
        ]
    }

    /// `out[j]` = 1 for each entry (allocation order) holding a page with `mask` set.
    fn entry_flags(&self, (buffer, index): (Buffer, u32), mask: u32) -> Vec<Command> {
        vec![
            Command::SetPipeline(self.pipelines.entry_flags),
            words([self.index.table, index, self.levels, mask]),
            dispatch(self.entries()),
            compute_barrier(&[buffer]),
        ]
    }

    /// Copies each dynamic page's static page into it (one group per page), before the dynamic
    /// casters draw.
    pub(crate) fn composite(&self, params: u32, pool: &[Buffer]) -> Vec<Command> {
        vec![
            Command::SetPipeline(self.pipelines.composite),
            words([params, self.index.copies]),
            Command::DispatchIndirect {
                buffer: self.counts,
                offset: 44,
            },
            pool_barrier(pool),
        ]
    }

    /// Marks the table from depth texture `depth` (bindless, `size` pixels) with parameter block
    /// `params` and depth views `views`.
    pub(crate) fn mark(
        &self,
        (depth, size): (u32, [u32; 2]),
        (params, views): (u32, u32),
    ) -> Vec<Command> {
        vec![
            Command::SetPipeline(self.pipelines.mark),
            words([depth, params, self.index.table, views, size[0], size[1]]),
            Command::Dispatch {
                groups: [size[0].div_ceil(8), size[1].div_ceil(8), 1],
            },
            compute_barrier(&[self.table]),
        ]
    }

    /// Decides residency, releases, evicts and allocates; gives resident pages under the dynamic
    /// casters' `movers` spheres a dynamic page (or, without room, marks them polluted); lists the
    /// pages to draw and writes the counts.
    pub(crate) fn allocate(&self, movers: (u32, u32), blocks: (u32, u32)) -> Vec<Command> {
        let (p, i, b) = (&self.pipelines, &self.index, &self.bufs);
        let (e, n) = (self.entries(), self.pool);
        let mut out = vec![
            Command::SetPipeline(p.flags),
            words([i.table, i.requests, self.levels]),
            dispatch(e),
            compute_barrier(&[b.requests]),
        ];
        out.extend_from_slice(self.scans.requests.commands());
        out.extend([
            Command::SetPipeline(p.update),
            words([
                i.table,
                i.requests,
                i.scanned,
                i.needs,
                i.pages,
                self.levels,
                n,
                self.keep_frames,
            ]),
            dispatch(e),
            compute_barrier(&[self.table, self.pages, b.needs]),
        ]);
        out.extend_from_slice(self.scans.needs.commands());
        out.extend(self.free_scan());
        out.extend([
            Command::SetPipeline(p.zero),
            words([i.histogram, HISTOGRAM_WORDS]),
            dispatch(HISTOGRAM_WORDS),
            compute_barrier(&[b.histogram]),
            Command::SetPipeline(p.age_histogram),
            words([i.pages, i.table, i.histogram, n]),
            dispatch(n),
            compute_barrier(&[b.histogram]),
            Command::SetPipeline(p.threshold),
            words([i.histogram, i.need_scan, e, i.free_scan, n]),
            Command::Dispatch { groups: [1, 1, 1] },
            compute_barrier(&[b.histogram]),
            Command::SetPipeline(p.age_flags),
            words([i.pages, i.table, i.histogram, i.eq, n]),
            dispatch(n),
            compute_barrier(&[b.eq]),
        ]);
        out.extend_from_slice(self.scans.eq.commands());
        out.extend([
            Command::SetPipeline(p.evict),
            words([i.pages, i.table, i.histogram, i.eq_scan, n]),
            dispatch(n),
            compute_barrier(&[self.table, self.pages]),
        ]);
        out.extend(self.free_scan());
        out.extend([
            Command::SetPipeline(p.free_list),
            words([i.free, i.free_scan, i.free_list, n]),
            dispatch(n),
            compute_barrier(&[b.free_list]),
            Command::SetPipeline(p.allocate),
            words([
                i.table,
                i.needs,
                i.need_scan,
                i.free_list,
                i.pages,
                self.levels,
            ]),
            dispatch(e),
            compute_barrier(&[self.table, self.pages]),
            Command::SetPipeline(p.zero),
            words([i.dynamic, e]),
            dispatch(e),
            compute_barrier(&[self.dynamic]),
        ]);
        out.extend(self.flag_spheres(movers, blocks, (FLAG_DYNAMIC, FLAG_RESIDENT)));
        out.extend(self.entry_flags((b.movers, i.movers), FLAG_DYNAMIC));
        out.extend_from_slice(self.scans.movers.commands());
        out.extend(self.free_scan());
        out.extend([
            Command::SetPipeline(p.free_list),
            words([i.free, i.free_scan, i.free_list, n]),
            dispatch(n),
            compute_barrier(&[b.free_list]),
            Command::SetPipeline(p.dynamic_allocate),
            words([
                i.table,
                i.movers,
                i.mover_scan,
                i.free_list,
                i.free_scan,
                i.pages,
                i.dynamic,
                i.copies,
                self.levels,
                n,
            ]),
            dispatch(e),
            compute_barrier(&[self.table, self.pages, self.dynamic, self.copies]),
        ]);
        out.extend(self.entry_flags((b.dirty, i.dirty), FLAG_DIRTY));
        out.extend_from_slice(self.scans.dirty.commands());
        out.extend([
            Command::SetPipeline(p.draw_list),
            words([i.table, i.dirty, i.dirty_scan, i.list, self.levels]),
            dispatch(e),
            Command::SetPipeline(p.counts),
            words([
                i.scanned,
                i.counts,
                e,
                n,
                i.need_scan,
                i.dirty_scan,
                i.histogram,
                i.free_scan,
                i.mover_scan,
            ]),
            Command::Dispatch { groups: [1, 1, 1] },
            indirect_barrier(&[
                self.table,
                self.pages,
                self.counts,
                self.list,
                self.dynamic,
                self.copies,
            ]),
        ]);
        out
    }

    /// The free flags of the pool's pages and their scan.
    fn free_scan(&self) -> Vec<Command> {
        let i = &self.index;
        let mut out = vec![
            Command::SetPipeline(self.pipelines.free_flags),
            words([i.pages, i.free, self.pool]),
            dispatch(self.pool),
            compute_barrier(&[self.bufs.free]),
        ];
        out.extend_from_slice(self.scans.free.commands());
        out
    }

    /// Clears the listed pages of the pool described by `params` (one group per page).
    pub(crate) fn clear(&self, params: u32, pool: &[Buffer]) -> Vec<Command> {
        let after = Access {
            stages: Stages::FRAGMENT | Stages::COMPUTE,
            access: AccessFlags::SHADER_READ | AccessFlags::SHADER_WRITE,
            layout: Layout::General,
        };
        let written = Access {
            stages: Stages::COMPUTE,
            access: AccessFlags::SHADER_WRITE,
            layout: Layout::General,
        };
        vec![
            Command::SetPipeline(self.pipelines.clear),
            words([params, self.index.list]),
            Command::DispatchIndirect {
                buffer: self.counts,
                offset: 12,
            },
            Command::Barrier {
                buffers: pool
                    .iter()
                    .map(|&buffer| BufferBarrier {
                        buffer,
                        before: written,
                        after,
                    })
                    .collect(),
                textures: Vec::new(),
            },
        ]
    }
}

/// Entry flags as the kernels take them (`PageFlags` in bits 16–23).
const FLAG_DIRTY: u32 = (PageFlags::DIRTY.bits() as u32) << 16;
const FLAG_DYNAMIC: u32 = (PageFlags::DYNAMIC.bits() as u32) << 16;
const FLAG_INVALIDATED: u32 = (PageFlags::INVALIDATED.bits() as u32) << 16;
/// `flag_spheres`' need: any entry holding a page.
const FLAG_HELD: u32 = 0;
/// `flag_spheres`' need: resident entries (requested and valid).
const FLAG_RESIDENT: u32 = ((PageFlags::REQUESTED.bits() | PageFlags::VALID.bits()) as u32) << 16;

/// Compute writes of the pool before page draws, composites and `forward`'s reads.
fn pool_barrier(pool: &[Buffer]) -> Command {
    let access = |stages, access| Access {
        stages,
        access,
        layout: Layout::General,
    };
    Command::Barrier {
        buffers: pool
            .iter()
            .map(|&buffer| BufferBarrier {
                buffer,
                before: access(Stages::COMPUTE, AccessFlags::SHADER_WRITE),
                after: access(
                    Stages::FRAGMENT | Stages::COMPUTE,
                    AccessFlags::SHADER_READ | AccessFlags::SHADER_WRITE,
                ),
            })
            .collect(),
        textures: Vec::new(),
    }
}

/// Compute writes of `buffers` before indirect dispatches and later shader reads.
pub(crate) fn indirect_barrier(buffers: &[Buffer]) -> Command {
    let written = Access {
        stages: Stages::COMPUTE,
        access: AccessFlags::SHADER_WRITE,
        layout: Layout::General,
    };
    let read = Access {
        stages: Stages::DRAW_INDIRECT | Stages::COMPUTE | Stages::FRAGMENT | Stages::VERTEX,
        access: AccessFlags::INDIRECT_READ | AccessFlags::SHADER_READ,
        layout: Layout::General,
    };
    Command::Barrier {
        buffers: buffers
            .iter()
            .map(|&buffer| BufferBarrier {
                buffer,
                before: written,
                after: read,
            })
            .collect(),
        textures: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use concordia_core::math::{IVec2, Mat4, Vec3, Vec4};
    use concordia_render_types::Projection;

    use super::*;
    use crate::test_gpu::{self, download, run_commands};
    use crate::vsm::frame::{Footprint, ParamsIn, depth_views, floor_level, params_rows};
    use crate::vsm::{
        Clipmap, PageEntry, PageFlags, PoolLayout, VsmSettings, level_for_footprint, table_slot,
    };

    const W: u32 = 64;
    const H: u32 = 48;

    fn pipelines(device: &dyn Device) -> VsmPipelines {
        let scan = ScanPipelines {
            groups: test_gpu::compute(device, "scan.slang", "scan_groups"),
            add: test_gpu::compute(device, "scan.slang", "scan_add"),
        };
        VsmPipelines::create(
            |entry| Ok(test_gpu::compute(device, "vsm_page.slang", entry)),
            scan,
        )
        .unwrap()
    }

    /// Depth of pixel `p`: 0 (sky) every 7th, else `(1 + j/1024) · 2^-e`, exact in half floats.
    fn depth_bits(p: u32) -> u16 {
        if p % 7 == 3 {
            return 0;
        }
        let (e, j) = (2 + p % 11, (p * 37) % 1024);
        (((15 - e) << 10) | j) as u16
    }

    fn half(bits: u16) -> f32 {
        match bits {
            0 => 0.0,
            b => (1.0 + f32::from(b & 0x3ff) / 1024.0) * 2f32.powi(i32::from(b >> 10) - 15),
        }
    }

    struct Setup {
        settings: VsmSettings,
        clipmap: Clipmap,
        camera: Mat4,
        eye: Vec3,
        per_metre: f32,
    }

    fn setup() -> Setup {
        let settings = VsmSettings {
            max_distance: 300.0,
            ..VsmSettings::default()
        };
        let eye = Vec3::new(3.0, 10.0, 20.0);
        let forward = (Vec3::ZERO - eye).normalize();
        let right = forward.cross(Vec3::Y).normalize();
        let rotation = concordia_core::math::Mat3::from_cols(right, right.cross(forward), -forward);
        let world = Mat4::from_translation(eye) * Mat4::from_mat3(rotation);
        let projection = Projection::Perspective {
            fov_y: 0.9,
            near: 0.1,
            far: 1000.0,
        };
        let camera = projection.matrix(W as f32 / H as f32).unwrap() * world.inverse();
        let clipmap = Clipmap::fit(
            Vec3::new(-0.3, -1.0, 0.2),
            eye,
            settings.levels,
            (-400.0, 400.0),
        );
        Setup {
            settings,
            clipmap,
            camera,
            eye,
            per_metre: 2.0 * (0.45f32).tan() / H as f32,
        }
    }

    /// The pages `vsm_mark` requests, computed on the CPU (the same formulas).
    fn mark_reference(s: &Setup, radius: f32) -> BTreeSet<u32> {
        let inv = s.camera.inverse();
        let floor = floor_level(s.settings.max_distance, s.settings.levels);
        let mut out = BTreeSet::new();
        for y in 0..H {
            for x in 0..W {
                let depth = half(depth_bits(y * W + x));
                if depth <= 0.0 {
                    continue;
                }
                let ndc = Vec4::new(
                    (x as f32 + 0.5) / W as f32 * 2.0 - 1.0,
                    1.0 - (y as f32 + 0.5) / H as f32 * 2.0,
                    depth,
                    1.0,
                );
                let h = inv * ndc;
                let world = h.truncate() / h.w;
                let distance = world.distance(s.eye);
                if distance > s.settings.max_distance {
                    continue;
                }
                let level = level_for_footprint(
                    distance * s.per_metre,
                    s.settings.resolution_bias,
                    s.settings.levels,
                );
                for level in [level, floor] {
                    let l = &s.clipmap.levels[level as usize];
                    let clip = l.view_proj * world.extend(1.0);
                    let t = Vec3::new(clip.x * 0.5 + 0.5, 0.5 - clip.y * 0.5, 0.0) * 16_384.0;
                    let lo = ((t.truncate() - radius) / 128.0).floor().as_ivec2();
                    let hi = ((t.truncate() + radius) / 128.0).floor().as_ivec2();
                    for py in lo.y..=hi.y {
                        for px in lo.x..=hi.x {
                            if px < 0 || py < 0 || px >= 128 || py >= 128 {
                                continue;
                            }
                            let page = l.world_page(concordia_core::math::IVec2::new(px, py));
                            out.insert(table_slot(level, page.into()));
                        }
                    }
                }
            }
        }
        out
    }

    /// `vsm_mark` requests exactly the pages a CPU reference finds over a depth buffer (each
    /// receiver's level and the coarse floor, widened by the PCF footprint), and nothing for the sky
    /// or beyond `max_distance`.
    #[test]
    fn page_marking_requests_only_pages_covering_visible_receivers() {
        let Some(device) = test_gpu::device() else {
            return;
        };
        let device = device.as_ref();
        let s = setup();
        let mut up = Uploads::new();
        let mut keep = Vec::new();
        let texels: Vec<u8> = (0..W * H)
            .flat_map(|p| {
                let b = depth_bits(p).to_le_bytes();
                [b[0], b[1], 0, 0, 0, 0, 0, 0]
            })
            .collect();
        let depth = up
            .texture(
                device,
                &concordia_assets::TextureData {
                    width: W,
                    height: H,
                    format: concordia_assets::TextureFormat::Rgba16Float,
                    mips: vec![texels],
                },
                "depth",
            )
            .unwrap();
        let params = params_rows(&ParamsIn {
            settings: &s.settings,
            clipmap: &s.clipmap,
            eye: s.eye,
            footprint: Footprint::PerMetre(s.per_metre),
            pcf_radius: 1,
            chunks: &[],
            inv_view_proj: s.camera.inverse(),
            previous: &origins(&s.clipmap),
            tables: [0, 0],
        });
        let params = up.buffer(device, &params, "params").unwrap();
        let views = up
            .buffer(device, &depth_views(&s.camera, &s.clipmap), "views")
            .unwrap();
        let levels = u32::from(s.settings.levels);
        let mut store = PageStore::default();
        let (store_bufs, _) = store.ensure(device, (levels, 4096)).unwrap();
        let shape = WorkShape {
            store: store_bufs,
            levels,
            pool: 4096,
            keep_frames: 60,
        };
        let work = PageWork::prepare(device, &up, pipelines(device), shape, &mut keep).unwrap();
        up.flush(device).unwrap();
        let index = |b: Buffer| device.buffer_index(b).unwrap().0;
        let depth = device.texture_index(depth).unwrap().0;
        let mut commands = work.begin(true, index(params));
        commands.extend(work.mark((depth, [W, H]), (index(params), index(views))));
        run_commands(device, commands);
        let table = download(device, work.table, u64::from(levels * LEVEL_ENTRIES) * 4);
        let got: BTreeSet<u32> = table
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                PageEntry::from_bits(**e)
                    .flags()
                    .contains(PageFlags::REQUESTED)
            })
            .map(|(i, _)| i as u32)
            .collect();
        let want = mark_reference(&s, 2.0);
        assert!(!want.is_empty());
        let floor = floor_level(s.settings.max_distance, s.settings.levels);
        assert!(
            want.iter().any(|e| e / LEVEL_ENTRIES == floor),
            "the floor is marked"
        );
        assert_eq!(got, want, "requested pages");
    }

    /// Each level's origin in `clipmap`.
    fn origins(clipmap: &Clipmap) -> Vec<IVec2> {
        clipmap.levels.iter().map(|l| l.origin_page()).collect()
    }

    /// Page work run frame by frame against one store, with requests written straight into the
    /// table after `begin` (in place of marking).
    struct Frames<'d> {
        device: &'d dyn Device,
        store: PageStore,
        levels: u32,
        pool: u32,
        keep_frames: u32,
        clipmap: Clipmap,
        previous: Vec<IVec2>,
        /// A real pool (only the composite tests need one).
        pool_buffers: Option<VsmPool>,
        /// The last frame's work and parameter block (for `composite`).
        last: Option<(PageWork, u32)>,
    }

    /// What a frame invalidates and which spheres hold dynamic casters.
    #[derive(Default, Clone, Copy)]
    struct Inputs<'a> {
        stale: &'a [Vec4],
        dynamic: &'a [Vec4],
    }

    /// One frame's store and outputs, read back.
    #[derive(Debug, PartialEq)]
    struct Out {
        table: Vec<u32>,
        pages: Vec<u32>,
        counts: Vec<u32>,
        list: Vec<u32>,
        /// Dynamic physical page + 1 per entry (0: none).
        dynamic: Vec<u32>,
    }

    impl Out {
        fn entry(&self, e: u32) -> PageEntry {
            PageEntry::from_bits(self.table[e as usize])
        }

        /// The entries holding a page, with it.
        fn held(&self) -> BTreeMap<u32, u16> {
            (0..self.table.len() as u32)
                .filter_map(|e| self.entry(e).physical().map(|p| (e, p)))
                .collect()
        }

        /// The resident entries (requested, with a page).
        fn resident(&self) -> BTreeSet<u32> {
            self.held()
                .into_keys()
                .filter(|e| self.entry(*e).flags().contains(PageFlags::REQUESTED))
                .collect()
        }
    }

    impl<'d> Frames<'d> {
        fn new(device: &'d dyn Device, levels: u32, pool: u32, keep_frames: u32) -> Self {
            let clipmap = Clipmap::fit(
                Vec3::new(-0.3, -1.0, 0.2),
                Vec3::new(1.0, 2.0, 3.0),
                levels as u8,
                (-50.0, 50.0),
            );
            Self {
                device,
                store: PageStore::default(),
                levels,
                pool,
                keep_frames,
                previous: origins(&clipmap),
                clipmap,
                pool_buffers: None,
                last: None,
            }
        }

        /// With a real pool (`pool` pages of depth).
        fn with_pool(mut self) -> Self {
            let mut pool = VsmPool::default();
            pool.ensure(self.device, PoolLayout::new(self.pool).unwrap())
                .unwrap();
            self.pool_buffers = Some(pool);
            self
        }

        /// The pool's buffers and bindless indices.
        fn chunks(&mut self) -> Vec<(Buffer, u32)> {
            match &mut self.pool_buffers {
                Some(p) => p
                    .ensure(self.device, PoolLayout::new(self.pool).unwrap())
                    .unwrap()
                    .to_vec(),
                None => Vec::new(),
            }
        }

        /// Runs the last frame's composite (static → dynamic copies).
        fn composite(&mut self) {
            let chunks = self.chunks();
            let pool: Vec<Buffer> = chunks.iter().map(|(b, _)| *b).collect();
            let (work, params) = self.last.clone().unwrap();
            run_commands(self.device, work.composite(params, &pool));
        }

        /// Words of physical page `page`.
        fn page_words(&mut self, page: u32) -> Vec<u32> {
            let chunks = self.chunks();
            let (buffer, _) = chunks[(page / 1024) as usize];
            let words = u64::from(crate::vsm::PAGE_TEXELS * crate::vsm::PAGE_TEXELS);
            let all = download(
                self.device,
                buffer,
                (u64::from(page % 1024) + 1) * words * 4,
            );
            all[((page % 1024) as u64 * words) as usize..].to_vec()
        }

        /// Writes `words` into physical page `page`.
        fn fill_page(&mut self, page: u32, words: &[u32]) {
            let chunks = self.chunks();
            let (buffer, _) = chunks[(page / 1024) as usize];
            let page_words = (crate::vsm::PAGE_TEXELS * crate::vsm::PAGE_TEXELS) as usize;
            let at = (page % 1024) as usize * page_words;
            let mut all = download(self.device, buffer, ((at + page_words) * 4) as u64);
            all[at..].copy_from_slice(words);
            let data: Vec<u8> = all.iter().flat_map(|w| w.to_le_bytes()).collect();
            let mut up = Uploads::new();
            up.write_into(self.device, buffer, &data).unwrap();
            up.flush(self.device).unwrap();
        }

        /// Moves the camera to `eye` for the next frame.
        fn move_to(&mut self, eye: Vec3) {
            self.clipmap = Clipmap::fit(
                Vec3::new(-0.3, -1.0, 0.2),
                eye,
                self.levels as u8,
                (-50.0, 50.0),
            );
        }

        fn frame(&mut self, reset: bool, requested: &[u32]) -> Out {
            self.frame_with(reset, requested, Inputs::default())
        }

        /// A frame that invalidates the pages under `inputs.stale` (world spheres) before the
        /// requests and gives the resident pages under `inputs.dynamic` a dynamic page.
        fn frame_with(&mut self, reset: bool, requested: &[u32], inputs: Inputs<'_>) -> Out {
            let device = self.device;
            let chunks: Vec<u32> = self.chunks().iter().map(|(_, i)| *i).collect();
            let settings = VsmSettings {
                levels: self.levels as u8,
                pool_pages: self.pool,
                ..VsmSettings::default()
            };
            let mut up = Uploads::new();
            let mut keep = Vec::new();
            let (store, fresh) = self.store.ensure(device, (self.levels, self.pool)).unwrap();
            let shape = WorkShape {
                store,
                levels: self.levels,
                pool: self.pool,
                keep_frames: self.keep_frames,
            };
            let work = PageWork::prepare(device, &up, pipelines(device), shape, &mut keep).unwrap();
            let index = |b: Buffer| device.buffer_index(b).unwrap().0;
            let params = params_rows(&ParamsIn {
                settings: &settings,
                clipmap: &self.clipmap,
                eye: Vec3::ZERO,
                footprint: Footprint::Fixed(0.01),
                pcf_radius: 1,
                chunks: &chunks,
                inv_view_proj: Mat4::IDENTITY,
                previous: &self.previous,
                tables: [index(work.table), index(work.dynamic)],
            });
            self.previous = origins(&self.clipmap);
            let params = up.buffer(device, &params, "params").unwrap();
            let views = up
                .buffer(
                    device,
                    &depth_views(&Mat4::IDENTITY, &self.clipmap),
                    "views",
                )
                .unwrap();
            let rows = |spheres: &[Vec4]| -> Vec<u8> {
                spheres
                    .iter()
                    .flat_map(|v| v.to_array())
                    .flat_map(f32::to_le_bytes)
                    .chain([0; 16])
                    .collect()
            };
            let stale = inputs.stale;
            let spheres = up.buffer(device, &rows(stale), "stale spheres").unwrap();
            let movers = up
                .buffer(device, &rows(inputs.dynamic), "dynamic spheres")
                .unwrap();
            up.flush(device).unwrap();
            let params = index(params);
            let mut commands = work.begin(reset || fresh, params);
            commands.extend(
                work.invalidate((index(spheres), stale.len() as u32), (params, index(views))),
            );
            run_commands(device, commands);
            let entries = self.levels * LEVEL_ENTRIES;
            let mut table = download(device, work.table, u64::from(entries) * 4);
            let flag = PageEntry::new(None, PageFlags::REQUESTED, 0).bits();
            requested.iter().for_each(|e| table[*e as usize] |= flag);
            let bytes: Vec<u8> = table.iter().flat_map(|w| w.to_le_bytes()).collect();
            let mut up = Uploads::new();
            up.write_into(device, work.table, &bytes).unwrap();
            up.flush(device).unwrap();
            let views = index(views);
            run_commands(
                device,
                work.allocate(
                    (index(movers), inputs.dynamic.len() as u32),
                    (params, views),
                ),
            );
            let pool = u64::from(self.pool) * 4;
            let out = Out {
                table: download(device, work.table, u64::from(entries) * 4),
                pages: download(device, work.pages, pool),
                counts: download(device, work.counts, u64::from(COUNT_WORDS) * 4),
                list: download(device, work.list, pool),
                dynamic: download(device, work.dynamic, u64::from(entries) * 4),
            };
            self.last = Some((work, params));
            out
        }
    }

    /// `count` entries of `level`, spread over its slots.
    fn spread(level: u32, count: u32, salt: u32) -> Vec<u32> {
        (0..count)
            .map(|i| level * LEVEL_ENTRIES + (i * 397 + salt * 7919) % LEVEL_ENTRIES)
            .collect()
    }

    /// Allocation order: coarse levels first, then by slot.
    fn alloc_order(entries: &[u32]) -> Vec<u32> {
        let mut order = entries.to_vec();
        order.sort_by_key(|e| (std::cmp::Reverse(e / LEVEL_ENTRIES), *e));
        order
    }

    /// On an empty store, requests get pages coarse levels first, then by slot, while the pool
    /// lasts; the rest stay requested without a page, the counts say so, and two runs agree.
    #[test]
    fn allocation_is_coarse_first_and_deterministic() {
        let Some(device) = test_gpu::device() else {
            return;
        };
        let device = device.as_ref();
        let requested: Vec<u32> = [(3u32, 10u32), (2, 20), (1, 30), (0, 40)]
            .iter()
            .flat_map(|&(l, n)| spread(l, n, 0))
            .collect();
        let run = || Frames::new(device, 4, 64, 60).frame(true, &requested);
        let out = run();
        // requested, resident, starved, clear (drawn, 1, 1), new, drawn, evicted, kept, used,
        // composite (0, 1, 1), dynamic, without room
        assert_eq!(
            out.counts,
            [100, 64, 36, 64, 1, 1, 64, 64, 0, 0, 64, 0, 1, 1, 0, 0]
        );
        for (k, &entry) in alloc_order(&requested).iter().enumerate() {
            let e = out.entry(entry);
            match k < 64 {
                true => {
                    assert_eq!(e.physical(), Some(k as u16), "entry {entry}");
                    assert_eq!(out.pages[k], entry + 1);
                    assert_eq!(out.list[k], k as u32);
                    assert!(e.flags().contains(PageFlags::VALID | PageFlags::DIRTY));
                }
                false => assert_eq!(e.physical(), None, "starved entry {entry}"),
            }
        }
        assert_eq!(run(), out, "deterministic");
    }

    /// A page requested again keeps its physical page and isn't drawn; nothing new is allocated.
    #[test]
    fn cached_pages_are_kept_and_not_drawn() {
        let Some(device) = test_gpu::device() else {
            return;
        };
        let mut frames = Frames::new(device.as_ref(), 4, 256, 60);
        let requested: Vec<u32> = (0..4).flat_map(|l| spread(l, 12, l)).collect();
        let first = frames.frame(true, &requested);
        let second = frames.frame(false, &requested);
        assert_eq!(first.counts[6], 48, "all new");
        assert_eq!(second.counts[..3], [48, 48, 0]);
        assert_eq!(
            (second.counts[3], second.counts[6], second.counts[7]),
            (0, 0, 0),
            "nothing new, nothing drawn"
        );
        assert_eq!(second.held(), first.held(), "same pages");
        for e in &requested {
            let f = second.entry(*e).flags();
            assert_eq!(f, PageFlags::REQUESTED | PageFlags::VALID, "entry {e}");
            assert_eq!(second.entry(*e).age(), 0);
        }
    }

    /// Unrequested pages are kept, ageing by one a frame, and freed once older than `keep_frames`;
    /// they are never resident while kept.
    #[test]
    fn unrequested_pages_age_and_free_after_keep_frames() {
        let Some(device) = test_gpu::device() else {
            return;
        };
        let mut frames = Frames::new(device.as_ref(), 2, 64, 2);
        let requested = spread(1, 10, 3);
        frames.frame(true, &requested);
        for age in 1..=2u8 {
            let out = frames.frame(false, &[]);
            assert_eq!(out.held().len(), 10, "kept at age {age}");
            assert!(out.resident().is_empty());
            assert_eq!((out.counts[9], out.counts[10]), (10, 10), "kept, used");
            for e in &requested {
                assert_eq!(out.entry(*e).age(), age);
                assert_eq!(out.entry(*e).flags(), PageFlags::VALID);
            }
        }
        let out = frames.frame(false, &[]);
        assert!(out.held().is_empty(), "freed past keep_frames");
        assert!(out.pages.iter().all(|p| *p == 0), "{:?}", out.pages);
        assert_eq!((out.counts[9], out.counts[10]), (0, 0));
    }

    /// When new pages outnumber the free ones, kept pages are evicted oldest first, ties by
    /// physical page; the new pages get the freed ones in page order; two runs agree.
    #[test]
    fn eviction_takes_the_oldest_first_and_is_deterministic() {
        let Some(device) = test_gpu::device() else {
            return;
        };
        let device = device.as_ref();
        let (a, b, c) = (spread(0, 32, 1), spread(1, 32, 2), spread(2, 20, 3));
        let (a1, a2) = a.split_at(16);
        let run = || {
            let mut frames = Frames::new(device, 4, 64, 60);
            let all: Vec<u32> = a.iter().chain(&b).copied().collect();
            let first = frames.frame(true, &all);
            frames.frame(false, &b);
            frames.frame(false, a1);
            // Ages now: a1 1, b 2, a2 3 (none requested); 20 new pages need room.
            (first, frames.frame(false, &c))
        };
        let (first, last) = run();
        let page_of = |e: &u32| first.entry(*e).physical().unwrap();
        let mut b_pages: Vec<u16> = b.iter().map(page_of).collect();
        b_pages.sort_unstable();
        let mut freed: BTreeSet<u16> = a2.iter().map(page_of).collect();
        freed.extend(&b_pages[..4]);
        assert_eq!(last.counts[8], 20, "evicted");
        let got: BTreeSet<u16> = c
            .iter()
            .map(|e| last.entry(*e).physical().unwrap())
            .collect();
        assert_eq!(got, freed, "oldest first, ties by page");
        for (k, e) in alloc_order(&c).iter().enumerate() {
            let want = *freed.iter().nth(k).unwrap();
            assert_eq!(last.entry(*e).physical(), Some(want), "page order");
        }
        for e in a2.iter().chain(&b) {
            let evicted = freed.contains(&page_of(e));
            assert_eq!(last.entry(*e).physical().is_none(), evicted, "entry {e}");
        }
        assert_eq!(run().1, last, "deterministic");
    }

    /// The same requests make the same entries resident whatever was kept before, also when the
    /// pool is too small for them all (residency is decided fresh, coarse levels first).
    #[test]
    fn residency_is_independent_of_history() {
        let Some(device) = test_gpu::device() else {
            return;
        };
        let device = device.as_ref();
        let wanted: Vec<u32> = (0..4).flat_map(|l| spread(l, 25, 5)).collect();
        let cold = Frames::new(device, 4, 64, 60).frame(true, &wanted);
        let mut frames = Frames::new(device, 4, 64, 60);
        frames.frame(true, &spread(0, 64, 9));
        frames.frame(
            false,
            &(0..4).flat_map(|l| spread(l, 15, l)).collect::<Vec<_>>(),
        );
        let warm = frames.frame(false, &wanted);
        assert_eq!(cold.counts[..3], [100, 64, 36]);
        assert_eq!(warm.counts[..3], cold.counts[..3]);
        assert_eq!(warm.resident(), cold.resident(), "resident set");
        assert_eq!(warm.counts[10], 64, "the pool is full");
    }

    /// When a level scrolls, a kept page whose slot now holds another world page is released; the
    /// rest keep their pages.
    #[test]
    fn age_releases_slots_scrolled_out() {
        let Some(device) = test_gpu::device() else {
            return;
        };
        let mut frames = Frames::new(device.as_ref(), 1, LEVEL_ENTRIES, 60);
        let all: Vec<u32> = (0..LEVEL_ENTRIES).collect();
        frames.frame(true, &all);
        let before = frames.clipmap.levels[0].origin_page();
        frames.move_to(Vec3::new(1.4, 2.0, 2.3));
        let after = frames.clipmap.levels[0].origin_page();
        assert_ne!(before, after, "the level scrolled");
        let out = frames.frame(false, &[]);
        let mut released = 0;
        for slot in 0..LEVEL_ENTRIES {
            let xy = [slot % PAGES_PER_SIDE, slot / PAGES_PER_SIDE];
            let gone =
                crate::vsm::slot_page(before.into(), xy) != crate::vsm::slot_page(after.into(), xy);
            released += u32::from(gone);
            assert_eq!(out.entry(slot).physical().is_none(), gone, "slot {xy:?}");
        }
        assert!(released > 0 && released < LEVEL_ENTRIES, "{released}");
        assert_eq!(out.counts[9], LEVEL_ENTRIES - released, "kept");
    }

    /// Invalidation flags exactly the held pages under each sphere's rect in each level: a
    /// requested one keeps its page and is redrawn, an unrequested one is released, the rest are
    /// untouched; a sphere outside every level touches nothing.
    #[test]
    fn invalidation_marks_exactly_the_pages_under_each_sphere() {
        let Some(device) = test_gpu::device() else {
            return;
        };
        let mut frames = Frames::new(device.as_ref(), 2, LEVEL_ENTRIES, 60);
        let all: Vec<u32> = (LEVEL_ENTRIES..2 * LEVEL_ENTRIES).collect();
        let first = frames.frame(true, &all);
        let stale = [
            Vec4::new(1.2, 0.5, 2.6, 0.4),
            Vec4::new(-0.5, 1.0, 4.0, 1.1),
            Vec4::new(9_000.0, 0.0, 0.0, 1.0),
        ];
        let l = &frames.clipmap.levels[1];
        let mut under = BTreeSet::new();
        for s in stale {
            for p in crate::vsm::testing::pages_under(&frames.clipmap, 1, s) {
                under.insert(table_slot(1, l.world_page(p).into()));
            }
        }
        assert!(under.len() > 20, "{}", under.len());
        let requested: Vec<u32> = all.iter().copied().filter(|e| e % 2 == 0).collect();
        let inputs = Inputs {
            stale: &stale,
            ..Inputs::default()
        };
        let out = frames.frame_with(false, &requested, inputs);
        let mut redrawn = 0;
        for &e in &all {
            let (before, after) = (first.entry(e), out.entry(e));
            match (under.contains(&e), e % 2 == 0) {
                (true, true) => {
                    redrawn += 1;
                    assert_eq!(after.physical(), before.physical(), "kept page {e}");
                    let want = PageFlags::REQUESTED
                        | PageFlags::VALID
                        | PageFlags::DIRTY
                        | PageFlags::INVALIDATED;
                    assert_eq!(after.flags(), want, "entry {e}");
                }
                (true, false) => assert_eq!(after.physical(), None, "released {e}"),
                (false, requested) => {
                    assert_eq!(after.physical(), before.physical(), "untouched {e}");
                    assert!(
                        !after
                            .flags()
                            .intersects(PageFlags::DIRTY | PageFlags::INVALIDATED)
                    );
                    assert_eq!(after.flags().contains(PageFlags::REQUESTED), requested);
                }
            }
        }
        assert_eq!((out.counts[6], out.counts[7]), (0, redrawn), "new, drawn");
    }

    /// The entries of `level` under `sphere` (CPU reference).
    fn entries_under(clipmap: &Clipmap, level: u32, sphere: Vec4) -> BTreeSet<u32> {
        let l = &clipmap.levels[level as usize];
        crate::vsm::testing::pages_under(clipmap, level, sphere)
            .into_iter()
            .map(|p| table_slot(level, l.world_page(p).into()))
            .collect()
    }

    /// Resident pages under a dynamic caster's sphere get a second (dynamic) page from the free
    /// ones, owned with the dynamic mark; the composite copies each static page into it; the next
    /// frame frees the dynamic pages and leaves the static ones alone.
    #[test]
    fn dynamic_pages_get_a_second_page_and_a_copy() {
        let Some(device) = test_gpu::device() else {
            return;
        };
        let mut frames = Frames::new(device.as_ref(), 2, 256, 60).with_pool();
        let mover = Vec4::new(1.2, 0.5, 2.6, 0.4);
        let under = entries_under(&frames.clipmap, 1, mover);
        let mut requested: Vec<u32> = under.iter().copied().collect();
        requested.extend(spread(1, 30, 4));
        requested.sort_unstable();
        requested.dedup();
        let first = frames.frame(true, &requested);
        let inputs = Inputs {
            dynamic: &[mover],
            ..Inputs::default()
        };
        let out = frames.frame_with(false, &requested, inputs);
        let statics: BTreeSet<u16> = first.held().into_values().collect();
        let mut seen = BTreeSet::new();
        for &e in &requested {
            let entry = out.entry(e);
            let d = out.dynamic[e as usize];
            match under.contains(&e) {
                true => {
                    assert!(entry.flags().contains(PageFlags::DYNAMIC), "entry {e}");
                    assert_ne!(d, 0, "entry {e} has a dynamic page");
                    let p = d - 1;
                    assert!(!statics.contains(&(p as u16)) && seen.insert(p), "page {p}");
                    assert_eq!(out.pages[p as usize], (e + 1) | DYNAMIC_OWNER);
                }
                false => {
                    assert!(!entry.flags().contains(PageFlags::DYNAMIC), "entry {e}");
                    assert_eq!(d, 0);
                }
            }
            assert_eq!(
                entry.physical(),
                first.entry(e).physical(),
                "static page kept"
            );
        }
        // requested, resident, starved, clear (3), new, drawn, evicted, kept, used, composite (3),
        // dynamic, fallback
        assert_eq!(out.counts[14..], [under.len() as u32, 0]);
        assert_eq!(
            out.counts[11],
            under.len() as u32,
            "one copy per dynamic page"
        );
        assert_eq!(
            out.counts[10],
            requested.len() as u32 + under.len() as u32,
            "used"
        );
        let pattern = |page: u32| -> Vec<u32> {
            (0..crate::vsm::PAGE_TEXELS * crate::vsm::PAGE_TEXELS)
                .map(|w| page * 100_003 + w)
                .collect()
        };
        for &e in &under {
            frames.fill_page(u32::from(out.entry(e).physical().unwrap()), &pattern(e));
        }
        frames.composite();
        for &e in &under {
            let d = out.dynamic[e as usize] - 1;
            assert!(frames.page_words(d) == pattern(e), "copied into page {d}");
        }
        let next = frames.frame(false, &requested);
        assert!(
            next.pages.iter().all(|o| o & DYNAMIC_OWNER == 0),
            "dynamic pages freed"
        );
        assert_eq!(next.held(), first.held(), "static pages untouched");
        assert!(next.dynamic.iter().all(|d| *d == 0));
    }

    /// Without room for a second page, the movers draw into the static page itself: it is redrawn
    /// this frame (with the static casters) and invalidated for the next, so it comes back clean.
    #[test]
    fn dynamic_without_room_draws_into_the_static_page() {
        let Some(device) = test_gpu::device() else {
            return;
        };
        let mut frames = Frames::new(device.as_ref(), 2, 64, 60);
        let mover = Vec4::new(1.2, 0.5, 2.6, 0.25);
        let under: Vec<u32> = entries_under(&frames.clipmap, 1, mover)
            .into_iter()
            .collect();
        assert!((9..=40).contains(&under.len()), "{}", under.len());
        let mut requested: Vec<u32> = under.clone();
        let fill = spread(1, 60, 6).into_iter().filter(|e| !under.contains(e));
        requested.extend(fill.take(60 - under.len()));
        requested.sort_unstable();
        frames.frame(true, &requested);
        let inputs = Inputs {
            dynamic: &[mover],
            ..Inputs::default()
        };
        let out = frames.frame_with(false, &requested, inputs);
        let room = 4usize;
        let order = alloc_order(&under);
        for (k, &e) in order.iter().enumerate() {
            let flags = out.entry(e).flags();
            match k < room {
                true => assert!(flags.contains(PageFlags::DYNAMIC), "entry {e}"),
                false => {
                    assert!(!flags.contains(PageFlags::DYNAMIC), "entry {e}");
                    assert!(
                        flags.contains(PageFlags::DIRTY | PageFlags::POLLUTED),
                        "entry {e}"
                    );
                }
            }
        }
        let fallback = (under.len() - room) as u32;
        assert_eq!(out.counts[14..], [room as u32, fallback]);
        assert_eq!(out.counts[7], fallback, "fallback pages drawn");
        let next = frames.frame(false, &requested);
        assert_eq!(next.counts[7], fallback, "polluted pages redrawn clean");
        for &e in &order[room..] {
            let flags = next.entry(e).flags();
            assert!(
                flags.contains(PageFlags::DIRTY | PageFlags::INVALIDATED),
                "entry {e}"
            );
            assert!(!flags.contains(PageFlags::POLLUTED));
        }
    }
}
