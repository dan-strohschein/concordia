//! The GPU side of virtual shadow maps' pages (M4 4.7a): the physical pool, and one frame's page
//! work (`vsm_page.slang`): zero the table, mark from the camera's depth, flag the requests in
//! allocation order (coarse levels first), scan them, allocate, count, and clear the pages to draw.

use std::num::NonZeroU64;

use concordia_rhi::{
    Access, AccessFlags, Buffer, BufferBarrier, BufferDesc, BufferUsage, Command, Device, Layout,
    MemoryLocation, Pipeline, Stages,
};

use super::pool::PoolLayout;
use super::table::PAGES_PER_SIDE;
use crate::RenderError;
use crate::gpu::Uploads;
use crate::scan::{Scan, ScanPipelines, compute_barrier};

/// Table entries per level.
const LEVEL_ENTRIES: u32 = PAGES_PER_SIDE * PAGES_PER_SIDE;
/// Words of the counts buffer: requested, allocated, starved, then the clear's dispatch.
pub(crate) const COUNT_WORDS: u32 = 6;

/// The page work's compute pipelines.
#[derive(Debug, Clone, Copy)]
pub(crate) struct VsmPipelines {
    pub(crate) zero: Pipeline,
    pub(crate) mark: Pipeline,
    pub(crate) flags: Pipeline,
    pub(crate) allocate: Pipeline,
    pub(crate) counts: Pipeline,
    pub(crate) clear: Pipeline,
    pub(crate) scan: ScanPipelines,
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
                    usage: BufferUsage::STORAGE | BufferUsage::BINDLESS | BufferUsage::COPY_SRC,
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

/// One frame's page buffers and their recorded work.
#[derive(Debug, Clone)]
pub(crate) struct PageWork {
    pipelines: VsmPipelines,
    levels: u32,
    pool: u32,
    /// The page table (levels × 128² entries).
    pub(crate) table: Buffer,
    requests: Buffer,
    /// Physical page → its table entry.
    pub(crate) pages: Buffer,
    /// [`COUNT_WORDS`] words.
    pub(crate) counts: Buffer,
    scan: Scan,
    index: Indices,
}

#[derive(Debug, Clone, Copy)]
struct Indices {
    table: u32,
    requests: u32,
    scanned: u32,
    pages: u32,
    counts: u32,
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
            usage: BufferUsage::STORAGE
                | BufferUsage::BINDLESS
                | BufferUsage::COPY_SRC
                | BufferUsage::COPY_DST
                | BufferUsage::INDIRECT,
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

impl PageWork {
    /// Allocates a frame's table, request, scan, page and count buffers (returned in `keep`) for
    /// `levels` levels over a pool of `pool` pages.
    pub(crate) fn prepare(
        device: &dyn Device,
        up: &Uploads,
        pipelines: VsmPipelines,
        (levels, pool): (u32, u32),
        keep: &mut Vec<Buffer>,
    ) -> Result<Self, RenderError> {
        let entries = levels * LEVEL_ENTRIES;
        let table = storage(device, up, entries, "vsm table")?;
        let requests = storage(device, up, entries, "vsm requests")?;
        let scanned = storage(device, up, entries + 1, "vsm request scan")?;
        let pages = storage(device, up, pool, "vsm pages")?;
        let counts = storage(device, up, COUNT_WORDS, "vsm counts")?;
        keep.extend([table, requests, scanned, pages, counts]);
        let scan = Scan::prepare(
            device,
            up,
            pipelines.scan,
            (requests, scanned),
            entries,
            keep,
        )?;
        let index = |b: Buffer| device.buffer_index(b).map(|i| i.0);
        let index = Indices {
            table: index(table)?,
            requests: index(requests)?,
            scanned: index(scanned)?,
            pages: index(pages)?,
            counts: index(counts)?,
        };
        Ok(Self {
            pipelines,
            levels,
            pool,
            table,
            requests,
            pages,
            counts,
            scan,
            index,
        })
    }

    fn entries(&self) -> u32 {
        self.levels * LEVEL_ENTRIES
    }

    /// Zeroes the table, then marks it from depth texture `depth` (bindless, `size` pixels) with
    /// parameter block `params` and depth views `views`.
    pub(crate) fn mark(
        &self,
        (depth, size): (u32, [u32; 2]),
        (params, views): (u32, u32),
    ) -> Vec<Command> {
        let p = &self.pipelines;
        vec![
            Command::SetPipeline(p.zero),
            words([self.index.table, self.entries()]),
            dispatch(self.entries()),
            compute_barrier(&[self.table]),
            Command::SetPipeline(p.mark),
            words([depth, params, self.index.table, views, size[0], size[1]]),
            Command::Dispatch {
                groups: [size[0].div_ceil(8), size[1].div_ceil(8), 1],
            },
            compute_barrier(&[self.table]),
        ]
    }

    /// Flags, scans and allocates the requests, then writes the counts.
    pub(crate) fn allocate(&self) -> Vec<Command> {
        let p = &self.pipelines;
        let i = &self.index;
        let mut out = vec![
            Command::SetPipeline(p.flags),
            words([i.table, i.requests, self.levels]),
            dispatch(self.entries()),
            compute_barrier(&[self.requests]),
        ];
        out.extend_from_slice(self.scan.commands());
        out.extend([
            Command::SetPipeline(p.allocate),
            words([
                i.table,
                i.requests,
                i.scanned,
                i.pages,
                self.levels,
                self.pool,
            ]),
            dispatch(self.entries()),
            Command::SetPipeline(p.counts),
            words([i.scanned, i.counts, self.entries(), self.pool]),
            Command::Dispatch { groups: [1, 1, 1] },
            indirect_barrier(&[self.table, self.pages, self.counts]),
        ]);
        out
    }

    /// Clears the allocated pages of the pool described by `params` (one group per page).
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
            words([params]),
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

/// Compute writes of `buffers` before indirect dispatches and later shader reads.
fn indirect_barrier(buffers: &[Buffer]) -> Command {
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
    use std::collections::BTreeSet;

    use concordia_core::math::{Mat4, Vec3, Vec4};
    use concordia_render_types::Projection;

    use super::*;
    use crate::test_gpu::{self, download, run_commands};
    use crate::vsm::frame::{Footprint, ParamsIn, depth_views, floor_level, params_rows};
    use crate::vsm::{Clipmap, PageEntry, PageFlags, VsmSettings, level_for_footprint, table_slot};

    const W: u32 = 64;
    const H: u32 = 48;

    fn pipelines(device: &dyn Device) -> VsmPipelines {
        let k = |entry| test_gpu::compute(device, "vsm_page.slang", entry);
        VsmPipelines {
            zero: k("vsm_zero"),
            mark: k("vsm_mark"),
            flags: k("vsm_request_flags"),
            allocate: k("vsm_allocate"),
            counts: k("vsm_counts"),
            clear: k("vsm_clear"),
            scan: ScanPipelines {
                groups: test_gpu::compute(device, "scan.slang", "scan_groups"),
                add: test_gpu::compute(device, "scan.slang", "scan_add"),
            },
        }
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
                            let page = l.origin_page() + concordia_core::math::IVec2::new(px, py);
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
        });
        let params = up.buffer(device, &params, "params").unwrap();
        let views = up
            .buffer(device, &depth_views(&s.camera, &s.clipmap), "views")
            .unwrap();
        let levels = u32::from(s.settings.levels);
        let work =
            PageWork::prepare(device, &up, pipelines(device), (levels, 4096), &mut keep).unwrap();
        up.flush(device).unwrap();
        let index = |b: Buffer| device.buffer_index(b).unwrap().0;
        let depth = device.texture_index(depth).unwrap().0;
        run_commands(
            device,
            work.mark((depth, [W, H]), (index(params), index(views))),
        );
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

    /// Requests are allocated coarse levels first, then by slot, while the pool lasts; the rest
    /// stay requested without a page, the counts say so, and two runs agree exactly.
    #[test]
    fn allocation_is_coarse_first_and_deterministic() {
        let Some(device) = test_gpu::device() else {
            return;
        };
        let device = device.as_ref();
        let levels = 4u32;
        let mut table = vec![0u32; (levels * LEVEL_ENTRIES) as usize];
        let mut requested = Vec::new();
        for (level, count) in [(3u32, 10u32), (2, 20), (1, 30), (0, 40)] {
            for i in 0..count {
                let entry = level * LEVEL_ENTRIES + (i * 397) % LEVEL_ENTRIES;
                table[entry as usize] = PageEntry::new(None, PageFlags::REQUESTED, 0).bits();
                requested.push(entry);
            }
        }
        let bytes: Vec<u8> = table.iter().flat_map(|w| w.to_le_bytes()).collect();
        let run = || {
            let mut up = Uploads::new();
            let mut keep = Vec::new();
            let work =
                PageWork::prepare(device, &up, pipelines(device), (levels, 64), &mut keep).unwrap();
            up.write_into(device, work.table, &bytes).unwrap();
            up.flush(device).unwrap();
            run_commands(device, work.allocate());
            (
                download(device, work.table, bytes.len() as u64),
                download(device, work.pages, 64 * 4),
                download(device, work.counts, u64::from(COUNT_WORDS) * 4),
            )
        };
        let (table, pages, counts) = run();
        assert_eq!(counts, [100, 64, 36, 64, 1, 1]);
        // Allocation order: level 3, 2, 1, then 0, each by slot.
        let mut order: Vec<u32> = requested.clone();
        order.sort_by_key(|e| (std::cmp::Reverse(e / LEVEL_ENTRIES), *e));
        for (k, &entry) in order.iter().enumerate() {
            let e = PageEntry::from_bits(table[entry as usize]);
            match k < 64 {
                true => {
                    assert_eq!(e.physical(), Some(k as u16), "entry {entry}");
                    assert_eq!(pages[k], entry);
                    assert!(e.flags().contains(PageFlags::VALID | PageFlags::DIRTY));
                }
                false => assert_eq!(e.physical(), None, "starved entry {entry}"),
            }
        }
        assert_eq!(run(), (table, pages, counts), "deterministic");
    }
}
