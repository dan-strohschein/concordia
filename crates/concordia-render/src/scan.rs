//! Deterministic GPU exclusive prefix sums (`scan.slang`; M4 4.4): the meshlet path's compaction,
//! and later page lists (4.7) and particles (M11). Integer sums in a fixed tree, so the result is
//! the same on every run and backend.

use std::num::NonZeroU64;

use concordia_rhi::{
    Access, AccessFlags, Buffer, BufferBarrier, BufferDesc, BufferUsage, Command, Device, Layout,
    MemoryLocation, Pipeline, Stages,
};

use crate::RenderError;
use crate::gpu::Uploads;

/// Values one group scans (`scan.slang`'s `SCAN_GROUP`).
pub(crate) const GROUP: u32 = 256;
/// Groups per dispatch row.
const ROW: u32 = 65_535;

/// The sums each level of a scan writes: `count` at the first, then one per group of the level
/// below, until one group holds a level.
pub(crate) fn levels(count: u32) -> Vec<u32> {
    let mut out = vec![count];
    let mut last = count;
    while last > GROUP {
        last = last.div_ceil(GROUP);
        out.push(last);
    }
    out
}

/// The dispatch covering `count` values, [`GROUP`] per group, in rows of at most [`ROW`] groups.
pub(crate) fn groups(count: u32) -> [u32; 3] {
    let groups = count.div_ceil(GROUP).max(1);
    match groups <= ROW {
        true => [groups, 1, 1],
        false => [ROW, groups.div_ceil(ROW), 1],
    }
}

/// The two scan pipelines (`scan_groups`, `scan_add`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct ScanPipelines {
    pub(crate) groups: Pipeline,
    pub(crate) add: Pipeline,
}

/// Push constants of `scan_groups` (mirrors `scan.slang`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanPush {
    /// Input buffer.
    pub values: u32,
    /// Output buffer (exclusive sums).
    pub out: u32,
    /// Group totals buffer.
    pub sums: u32,
    /// Sums to write.
    pub count: u32,
    /// Inputs to read (later ones read 0).
    pub valid: u32,
}

concordia_shaders::shader_struct!(ScanPush {
    values,
    out,
    sums,
    count,
    valid
});

/// Push constants of `scan_add` (mirrors `scan.slang`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanAddPush {
    /// The level's sums.
    pub out: u32,
    /// The scanned totals of its groups.
    pub bases: u32,
    /// Sums in the level.
    pub count: u32,
}

concordia_shaders::shader_struct!(ScanAddPush { out, bases, count });

fn bytes<const N: usize>(words: [u32; N]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// A prepared scan of `valid` values into `valid + 1` exclusive sums (the last is the total).
#[derive(Debug, Clone)]
pub(crate) struct Scan {
    commands: Vec<Command>,
}

impl Scan {
    /// Allocates the scan's per-level buffers (returned in `keep`) and records its dispatches;
    /// `values` and `out` are bindless buffers, `out` holding at least `valid + 1` words.
    pub(crate) fn prepare(
        device: &dyn Device,
        up: &Uploads,
        pipelines: ScanPipelines,
        (values, out): (Buffer, Buffer),
        valid: u32,
        keep: &mut Vec<Buffer>,
    ) -> Result<Self, RenderError> {
        let count = valid.checked_add(1).ok_or_else(|| {
            RenderError::Gpu(concordia_rhi::RhiError::invalid(
                "scan",
                "more than 2^32 - 1 values",
            ))
        })?;
        let index = |b: Buffer| device.buffer_index(b).map(|i| i.0);
        let counts = levels(count);
        // Level k scans `inputs[k]` into `outs[k]`, its group totals into `sums[k]`; level k + 1
        // scans those totals.
        let mut outs = vec![out];
        let mut sums = Vec::with_capacity(counts.len());
        for (k, &c) in counts.iter().enumerate() {
            let totals = scratch(device, up, c.div_ceil(GROUP), "scan totals")?;
            sums.push(totals);
            if k + 1 < counts.len() {
                outs.push(scratch(device, up, counts[k + 1], "scan level")?);
            }
        }
        keep.extend(outs[1..].iter().chain(&sums));
        let mut commands = Vec::new();
        for (k, &c) in counts.iter().enumerate() {
            let (input, valid) = match k {
                0 => (values, valid),
                _ => (sums[k - 1], c),
            };
            commands.extend([
                Command::SetPipeline(pipelines.groups),
                Command::PushConstants(bytes([
                    index(input)?,
                    index(outs[k])?,
                    index(sums[k])?,
                    c,
                    valid,
                ])),
                Command::Dispatch { groups: groups(c) },
                compute_barrier(&[outs[k], sums[k]]),
            ]);
        }
        for k in (0..counts.len() - 1).rev() {
            commands.extend([
                Command::SetPipeline(pipelines.add),
                Command::PushConstants(bytes([index(outs[k])?, index(outs[k + 1])?, counts[k]])),
                Command::Dispatch {
                    groups: groups(counts[k]),
                },
                compute_barrier(&[outs[k]]),
            ]);
        }
        Ok(Self { commands })
    }

    /// The scan's dispatches and barriers; `out` is ready for compute reads after them.
    pub(crate) fn commands(&self) -> &[Command] {
        &self.commands
    }
}

/// Shader writes of `buffers` before later compute reads.
pub(crate) fn compute_barrier(buffers: &[Buffer]) -> Command {
    let written = Access {
        stages: Stages::COMPUTE,
        access: AccessFlags::SHADER_WRITE,
        layout: Layout::General,
    };
    let read = Access {
        stages: Stages::COMPUTE,
        access: AccessFlags::SHADER_READ | AccessFlags::SHADER_WRITE,
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

/// A GPU-only storage buffer of `words` words for a scan level.
fn scratch(
    device: &dyn Device,
    up: &Uploads,
    words: u32,
    label: &str,
) -> Result<Buffer, RenderError> {
    Ok(up.alloc(
        device,
        &BufferDesc {
            size: NonZeroU64::new(u64::from(words) * 4).unwrap_or(NonZeroU64::MIN),
            usage: BufferUsage::STORAGE | BufferUsage::BINDLESS | BufferUsage::COPY_SRC,
            memory: MemoryLocation::GpuOnly,
            label: label.into(),
        },
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_gpu;
    use proptest::prelude::*;

    #[test]
    fn levels_shrink_by_groups_until_one_holds_them() {
        assert_eq!(levels(1), vec![1]);
        assert_eq!(levels(256), vec![256]);
        assert_eq!(levels(257), vec![257, 2]);
        assert_eq!(levels(65_536), vec![65_536, 256]);
        assert_eq!(levels(65_537), vec![65_537, 257, 2]);
        assert_eq!(levels(u32::MAX), vec![u32::MAX, 16_777_216, 65_536, 256]);
    }

    #[test]
    fn groups_cover_every_value_in_rows() {
        assert_eq!(groups(1), [1, 1, 1]);
        assert_eq!(groups(256), [1, 1, 1]);
        assert_eq!(groups(257), [2, 1, 1]);
        let row = ROW * GROUP;
        assert_eq!(groups(row), [ROW, 1, 1]);
        assert_eq!(groups(row + 1), [ROW, 2, 1]);
    }

    /// Exclusive sums of `values` on the GPU (`values.len() + 1` of them).
    fn pipelines(device: &dyn Device) -> ScanPipelines {
        ScanPipelines {
            groups: test_gpu::compute(device, "scan.slang", "scan_groups"),
            add: test_gpu::compute(device, "scan.slang", "scan_add"),
        }
    }

    fn gpu_scan(device: &dyn Device, pipelines: ScanPipelines, values: &[u32]) -> Vec<u32> {
        gpu_scan_prefix(device, pipelines, values, values.len() as u32)
    }

    /// Exclusive sums of the first `valid` of `values` (the buffer holds them all).
    fn gpu_scan_prefix(
        device: &dyn Device,
        pipelines: ScanPipelines,
        values: &[u32],
        valid: u32,
    ) -> Vec<u32> {
        let mut up = Uploads::new();
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let input = up.buffer(device, &bytes, "values").unwrap();
        let out = scratch(device, &up, valid + 1, "sums").unwrap();
        let mut keep = Vec::new();
        let scan = Scan::prepare(device, &up, pipelines, (input, out), valid, &mut keep).unwrap();
        up.flush(device).unwrap();
        test_gpu::run_commands(device, scan.commands().iter().cloned());
        test_gpu::download(device, out, (u64::from(valid) + 1) * 4)
    }

    /// Values past `valid` in a longer buffer (a pooled one) are never summed.
    #[test]
    fn scan_reads_only_its_valid_values() {
        let Some(device) = test_gpu::device() else {
            return;
        };
        let pipelines = pipelines(device.as_ref());
        for valid in [1u32, 255, 256, 300] {
            let values = vec![1u32; valid as usize + 64];
            let got = gpu_scan_prefix(device.as_ref(), pipelines, &values, valid);
            assert_eq!(
                got,
                cpu_scan(&values[..valid as usize]),
                "{valid} of {}",
                values.len()
            );
        }
    }

    fn cpu_scan(values: &[u32]) -> Vec<u32> {
        let mut sum = 0;
        let mut out: Vec<u32> = values
            .iter()
            .map(|v| {
                let before = sum;
                sum += v;
                before
            })
            .collect();
        out.push(sum);
        out
    }

    #[test]
    fn scan_matches_a_cpu_prefix_sum_across_group_and_level_edges() {
        let Some(device) = test_gpu::device() else {
            return;
        };
        let pipelines = pipelines(device.as_ref());
        for n in [1usize, 255, 256, 257, 511, 65_535, 65_536, 65_537, 200_000] {
            let values: Vec<u32> = (0..n as u32)
                .map(|i| i.wrapping_mul(2_654_435_761) >> 29)
                .collect();
            assert_eq!(
                gpu_scan(device.as_ref(), pipelines, &values),
                cpu_scan(&values),
                "{n} values"
            );
        }
    }

    /// One device and its pipelines for every proptest case.
    fn shared() -> Option<&'static (std::sync::Arc<dyn Device>, ScanPipelines)> {
        static SHARED: std::sync::OnceLock<Option<(std::sync::Arc<dyn Device>, ScanPipelines)>> =
            std::sync::OnceLock::new();
        SHARED
            .get_or_init(|| {
                test_gpu::device().map(|d| {
                    let p = pipelines(d.as_ref());
                    (d, p)
                })
            })
            .as_ref()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(16))]
        #[test]
        fn scan_matches_a_cpu_prefix_sum(values in prop::collection::vec(0u32..4, 1..3000)) {
            let Some((device, pipelines)) = shared() else {
                return Ok(());
            };
            prop_assert_eq!(gpu_scan(device.as_ref(), *pipelines, &values), cpu_scan(&values));
        }
    }
}
