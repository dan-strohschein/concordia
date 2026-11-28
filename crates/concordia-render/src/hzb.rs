//! The hierarchical depth buffer (`hzb.slang`; M4 4.4b, ADR-0100): an `f32` mip chain in one
//! buffer, level after level, built from the frame's single-sample (min-resolved) depth. With
//! reverse-Z the farthest depth is the smallest value, so each texel is the min of its parents:
//! level 0 is ⌈w/2⌉ × ⌈h/2⌉, and each level halves rounding up, so texel (x, y) of level L covers
//! exactly the pixels 2^(L+1)·x … 2^(L+1)·(x+1) − 1 (clipped at the edges). Greene, Kass & Miller
//! 1993 (hierarchical Z); the projected-sphere test that reads it is `cull::project_sphere`.

use std::num::{NonZeroU32, NonZeroU64};

use concordia_rhi::{Buffer, BufferDesc, BufferUsage, Command, Device, MemoryLocation, Pipeline};

use crate::RenderError;
use crate::gpu::Uploads;
use crate::scan::compute_barrier;

/// Threads per group side of `hzb_build` (8 × 8).
const GROUP: u32 = 8;

/// The sizes of an HZB over a `width` × `height` depth: level 0 is half of it rounding up, and
/// each level halves the one before rounding up, down to 1 × 1.
pub(crate) fn level_sizes(width: NonZeroU32, height: NonZeroU32) -> Vec<[u32; 2]> {
    let mut size = [width.get().div_ceil(2), height.get().div_ceil(2)];
    let mut out = vec![size];
    while size != [1, 1] {
        size = size.map(|n| n.div_ceil(2));
        out.push(size);
    }
    out
}

/// Where an HZB's levels lie in its buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HzbLayout {
    /// The depth's size in pixels.
    pub(crate) screen: [u32; 2],
    /// Each level's size, finest first.
    pub(crate) levels: Vec<[u32; 2]>,
    /// Each level's first texel in the buffer.
    pub(crate) offsets: Vec<u32>,
}

impl HzbLayout {
    /// The layout over a `width` × `height` depth.
    pub(crate) fn new(width: NonZeroU32, height: NonZeroU32) -> Self {
        let levels = level_sizes(width, height);
        let mut offsets = Vec::with_capacity(levels.len());
        let mut next = 0;
        for [w, h] in &levels {
            offsets.push(next);
            next += w * h;
        }
        Self {
            screen: [width.get(), height.get()],
            levels,
            offsets,
        }
    }

    /// Texels in every level together.
    pub(crate) fn texels(&self) -> u32 {
        self.levels.iter().map(|[w, h]| w * h).sum()
    }

    #[cfg(test)] // The CPU reference of `meshlet.slang`'s level choice.
    /// The finest level at which the pixel rectangle `lo..=hi` spans at most 2 × 2 texels.
    pub(crate) fn level_for(&self, lo: [u32; 2], hi: [u32; 2]) -> usize {
        debug_assert!(lo[0] <= hi[0] && lo[1] <= hi[1], "{lo:?}..={hi:?}");
        let fits = |l: usize| (0..2).all(|a| (hi[a] >> (l + 1)) - (lo[a] >> (l + 1)) <= 1);
        (0..self.levels.len())
            .find(|&l| fits(l))
            .unwrap_or(self.levels.len() - 1)
    }
}

/// Push constants of `hzb_build` (mirrors `hzb.slang`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HzbPush {
    /// The source depth texture (level 0 reads it).
    pub depth: u32,
    /// The HZB buffer.
    pub hzb: u32,
    /// 1 when the source is the depth texture, 0 for the level before in the buffer.
    pub from_depth: u32,
    /// The source level's first texel (buffer sources).
    pub src_offset: u32,
    /// The source's width.
    pub src_width: u32,
    /// The source's height.
    pub src_height: u32,
    /// The level's first texel.
    pub dst_offset: u32,
    /// The level's width.
    pub dst_width: u32,
    /// The level's height.
    pub dst_height: u32,
}

concordia_shaders::shader_struct!(HzbPush {
    depth,
    hzb,
    from_depth,
    src_offset,
    src_width,
    src_height,
    dst_offset,
    dst_width,
    dst_height
});

impl HzbPush {
    fn bytes(&self) -> Vec<u8> {
        [
            self.depth,
            self.hzb,
            self.from_depth,
            self.src_offset,
            self.src_width,
            self.src_height,
            self.dst_offset,
            self.dst_width,
            self.dst_height,
        ]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect()
    }
}

/// A frame's HZB: its buffer and layout.
#[derive(Debug, Clone)]
pub(crate) struct Hzb {
    pub(crate) buffer: Buffer,
    pub(crate) layout: HzbLayout,
}

impl Hzb {
    /// Allocates the HZB over `layout` (returned in `keep`).
    pub(crate) fn prepare(
        device: &dyn Device,
        up: &Uploads,
        layout: HzbLayout,
        keep: &mut Vec<Buffer>,
    ) -> Result<Self, RenderError> {
        let buffer = up.alloc(
            device,
            &BufferDesc {
                size: NonZeroU64::new(u64::from(layout.texels()) * 4).unwrap_or(NonZeroU64::MIN),
                usage: BufferUsage::STORAGE | BufferUsage::BINDLESS | BufferUsage::COPY_SRC,
                memory: MemoryLocation::GpuOnly,
                label: "hzb".into(),
            },
        )?;
        keep.push(buffer);
        Ok(Self { buffer, layout })
    }

    /// The build from the bindless single-sample depth texture `depth`, `hzb` being the buffer's
    /// bindless index: one dispatch per level, each followed by a barrier, so the buffer is ready
    /// for compute reads after them.
    pub(crate) fn commands(&self, pipeline: Pipeline, (depth, hzb): (u32, u32)) -> Vec<Command> {
        let layout = &self.layout;
        let mut commands = Vec::with_capacity(layout.levels.len() * 4);
        let mut src = (0, layout.screen);
        for (level, &[w, h]) in layout.levels.iter().enumerate() {
            let push = HzbPush {
                depth,
                hzb,
                from_depth: u32::from(level == 0),
                src_offset: src.0,
                src_width: src.1[0],
                src_height: src.1[1],
                dst_offset: layout.offsets[level],
                dst_width: w,
                dst_height: h,
            };
            commands.extend([
                Command::SetPipeline(pipeline),
                Command::PushConstants(push.bytes()),
                Command::Dispatch {
                    groups: [w.div_ceil(GROUP), h.div_ceil(GROUP), 1],
                },
                compute_barrier(&[self.buffer]),
            ]);
            src = (layout.offsets[level], [w, h]);
        }
        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_gpu;
    use concordia_rhi::{Format, SampleCount, TextureDesc, TextureUsage};
    use proptest::prelude::*;

    fn nz(v: u32) -> NonZeroU32 {
        NonZeroU32::new(v).unwrap()
    }

    #[test]
    fn hzb_level_sizes_halve_rounding_up() {
        assert_eq!(level_sizes(nz(1), nz(1)), vec![[1, 1]]);
        assert_eq!(level_sizes(nz(2), nz(2)), vec![[1, 1]]);
        assert_eq!(level_sizes(nz(5), nz(3)), vec![[3, 2], [2, 1], [1, 1]]);
        assert_eq!(
            level_sizes(nz(320), nz(180)),
            vec![
                [160, 90],
                [80, 45],
                [40, 23],
                [20, 12],
                [10, 6],
                [5, 3],
                [3, 2],
                [2, 1],
                [1, 1]
            ]
        );
        let full_hd = level_sizes(nz(1920), nz(1080));
        assert_eq!((full_hd[0], full_hd.len()), ([960, 540], 11));
        let layout = HzbLayout::new(nz(5), nz(3));
        assert_eq!(layout.offsets, vec![0, 6, 8]);
        assert_eq!(layout.texels(), 9);
    }

    proptest! {
        /// The chosen level spans at most 2 × 2 texels, and the level below it (finer) would not.
        #[test]
        fn hzb_level_choice_covers_the_rectangle_in_two_by_two(
            size in (1u32..2000, 1u32..2000),
            a in (0.0f64..1.0, 0.0f64..1.0),
            b in (0.0f64..1.0, 0.0f64..1.0),
        ) {
            let layout = HzbLayout::new(nz(size.0), nz(size.1));
            let px = |t: f64, n: u32| ((t * f64::from(n)) as u32).min(n - 1);
            let (x0, x1) = (px(a.0.min(b.0), size.0), px(a.0.max(b.0), size.0));
            let (y0, y1) = (px(a.1.min(b.1), size.1), px(a.1.max(b.1), size.1));
            let level = layout.level_for([x0, y0], [x1, y1]);
            prop_assert!(level < layout.levels.len());
            let span = |l: usize, lo: u32, hi: u32| (hi >> (l + 1)) - (lo >> (l + 1)) + 1;
            prop_assert!(span(level, x0, x1) <= 2 && span(level, y0, y1) <= 2);
            if level > 0 {
                prop_assert!(span(level - 1, x0, x1) > 2 || span(level - 1, y0, y1) > 2);
            }
        }
    }

    /// The HZB of a depth texture, read back level by level.
    fn gpu_hzb(
        device: &dyn Device,
        pipeline: Pipeline,
        (w, h): (u32, u32),
        depth: &[f32],
    ) -> Vec<f32> {
        let texture = device
            .create_texture(&TextureDesc {
                width: nz(w),
                height: nz(h),
                mip_levels: nz(1),
                samples: SampleCount::X1,
                format: Format::Depth32Float,
                usage: TextureUsage::SAMPLED | TextureUsage::BINDLESS | TextureUsage::COPY_DST,
                label: "depth".into(),
            })
            .unwrap();
        let bytes: Vec<u8> = depth.iter().flat_map(|d| d.to_le_bytes()).collect();
        // A copy source: Vulkan checks the usage (Metal doesn't).
        let staging = device
            .create_buffer(&BufferDesc {
                size: NonZeroU64::new(bytes.len() as u64).unwrap(),
                usage: BufferUsage::COPY_SRC,
                memory: MemoryLocation::Upload,
                label: "depth values".into(),
            })
            .unwrap();
        device.write_buffer(staging, 0, &bytes).unwrap();
        test_gpu::run_commands(
            device,
            [Command::CopyBufferToTexture {
                src: staging,
                offset: 0,
                bytes_per_row: w * 4,
                dst: texture,
                mip: 0,
            }],
        );
        let layout = HzbLayout::new(nz(w), nz(h));
        let mut keep = Vec::new();
        let index = device.texture_index(texture).unwrap().0;
        let hzb = Hzb::prepare(device, &Uploads::new(), layout.clone(), &mut keep).unwrap();
        let buffer = device.buffer_index(hzb.buffer).unwrap().0;
        test_gpu::run_commands(device, hzb.commands(pipeline, (index, buffer)));
        let words = test_gpu::download(device, hzb.buffer, u64::from(hzb.layout.texels()) * 4);
        device.destroy_texture(texture);
        device.destroy_buffer(staging);
        words.into_iter().map(f32::from_bits).collect()
    }

    /// Each texel equals the min (the farthest depth with reverse-Z) of the parents it covers,
    /// exactly, at every level, for odd and even sizes.
    #[test]
    fn hzb_mips_are_conservative_farthest_depth() {
        let Some(device) = test_gpu::device() else {
            return;
        };
        let pipeline = test_gpu::compute(device.as_ref(), "hzb.slang", "hzb_build");
        for (w, h) in [(1, 1), (2, 2), (5, 3), (37, 23), (64, 64), (320, 180)] {
            let depth: Vec<f32> = (0..w * h)
                .map(|i: u32| (i.wrapping_mul(2_654_435_761) >> 8) as f32 / 16_777_216.0)
                .collect();
            let got = gpu_hzb(device.as_ref(), pipeline, (w, h), &depth);
            let layout = HzbLayout::new(nz(w), nz(h));
            let mut parent: (Vec<f32>, [u32; 2]) = (depth.clone(), [w, h]);
            for (level, &[lw, lh]) in layout.levels.iter().enumerate() {
                let at = layout.offsets[level] as usize;
                let mut expected = Vec::with_capacity((lw * lh) as usize);
                for y in 0..lh {
                    for x in 0..lw {
                        let mut m = f32::INFINITY;
                        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            let (sx, sy) = (2 * x + dx, 2 * y + dy);
                            if sx < parent.1[0] && sy < parent.1[1] {
                                m = m.min(parent.0[(sy * parent.1[0] + sx) as usize]);
                            }
                        }
                        expected.push(m);
                    }
                }
                assert_eq!(
                    &got[at..at + expected.len()],
                    &expected[..],
                    "{w}×{h}, level {level}"
                );
                parent = (expected, [lw, lh]);
            }
        }
    }
}
