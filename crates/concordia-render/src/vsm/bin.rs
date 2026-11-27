//! Binning casters to pages (M4 4.7a2): each caster (a non-meshlet instance in a level, or a
//! surviving meshlet of the levels' plan) is paired with every page due to be drawn under its
//! bounding sphere; the pairs, from a deterministic scan, drive the page pass, so its cost follows
//! the pages drawn rather than the levels' virtual area.

use std::num::NonZeroU64;

use concordia_core::math::Vec4;
use concordia_rhi::{Buffer, BufferDesc, BufferUsage, Command, Device, MemoryLocation, Pipeline};

use crate::RenderError;
use crate::frame::{push_rows, push_rows_u32};
use crate::gpu::Uploads;
use crate::scan::{Scan, ScanPipelines, compute_barrier};

/// Bytes of one pair: (caster, level, page in the level, physical page).
pub(crate) const PAIR_BYTES: u64 = 16;

/// The binning pipelines.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BinPipelines {
    pub(crate) unit_count: Pipeline,
    pub(crate) unit_emit: Pipeline,
    pub(crate) unit_args: Pipeline,
    pub(crate) meshlet_count: Pipeline,
    pub(crate) meshlet_emit: Pipeline,
    pub(crate) meshlet_args: Pipeline,
    pub(crate) meshlet_ranks: Pipeline,
    /// `vsm_zero` (the counts past the survivors).
    pub(crate) zero: Pipeline,
    pub(crate) scan: ScanPipelines,
}

/// What binning the levels' meshlet survivors reads (`MeshletJob::bin_inputs`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct BinInputs {
    pub(crate) buckets: u32,
    /// The compacted entries (a buffer: the page draw reads it too).
    pub(crate) entries: Buffer,
    /// The cull's arguments (survivors per bucket).
    pub(crate) args: Buffer,
    pub(crate) total: u32,
    pub(crate) instances: u32,
    pub(crate) meshes: u32,
}

/// One frame's binning of the levels' meshlet survivors, recorded: one draw of every pair.
#[derive(Debug, Clone)]
pub(crate) struct MeshletBins {
    pub(crate) pairs: Buffer,
    /// One `DrawIndirect` record.
    pub(crate) args: Buffer,
    /// [pairs wanted, pairs kept].
    pub(crate) totals: Buffer,
    /// The survivors the pairs name.
    pub(crate) entries: Buffer,
    commands: Vec<Command>,
}

impl MeshletBins {
    /// Allocates the ranks, counts, scan and room for `capacity` pairs of `vertices` vertices
    /// each, over the plan's `buckets` buckets.
    pub(crate) fn prepare(
        device: &dyn Device,
        up: &mut Uploads,
        pipelines: BinPipelines,
        (input, buckets, capacity, vertices): (BinInputs, u32, u32, u32),
        (params, views, table): (u32, u32, u32),
        keep: &mut Vec<Buffer>,
    ) -> Result<Self, RenderError> {
        let total = input.total;
        let ranks = storage(device, up, u64::from(buckets + 1) * 4, "vsm meshlet ranks")?;
        let dispatch = storage(device, up, 12, "vsm meshlet dispatch")?;
        let counts = storage(device, up, u64::from(total) * 4, "vsm meshlet counts")?;
        let scanned = storage(device, up, u64::from(total + 1) * 4, "vsm meshlet scan")?;
        let pairs = storage(
            device,
            up,
            u64::from(capacity.max(1)) * PAIR_BYTES,
            "vsm meshlet pairs",
        )?;
        let args = storage(device, up, 16, "vsm meshlet arguments")?;
        let totals = storage(device, up, 8, "vsm meshlet totals")?;
        keep.extend([ranks, dispatch, counts, scanned, pairs, args, totals]);
        let scan = Scan::prepare(device, up, pipelines.scan, (counts, scanned), total, keep)?;
        let index = |b: Buffer| device.buffer_index(b).map(|i| i.0);
        let bin_push = words(&[
            input.buckets,
            buckets,
            index(ranks)?,
            index(input.entries)?,
            input.instances,
            input.meshes,
            params,
            views,
            table,
            index(counts)?,
            index(scanned)?,
            index(pairs)?,
            capacity,
        ]);
        let per_survivor = Command::DispatchIndirect {
            buffer: dispatch,
            offset: 0,
        };
        let mut commands = vec![
            Command::SetPipeline(pipelines.zero),
            words(&[index(counts)?, total]),
            Command::Dispatch {
                groups: crate::scan::groups(total),
            },
            Command::SetPipeline(pipelines.meshlet_ranks),
            words(&[
                input.buckets,
                index(input.args)?,
                buckets,
                index(ranks)?,
                index(dispatch)?,
            ]),
            Command::Dispatch { groups: [1, 1, 1] },
            super::gpu::indirect_barrier(&[ranks, dispatch, counts]),
            Command::SetPipeline(pipelines.meshlet_count),
            bin_push.clone(),
            per_survivor.clone(),
            compute_barrier(&[counts]),
        ];
        commands.extend_from_slice(scan.commands());
        commands.extend([
            Command::SetPipeline(pipelines.meshlet_emit),
            bin_push,
            per_survivor,
            Command::SetPipeline(pipelines.meshlet_args),
            words(&[
                index(scanned)?,
                total,
                index(args)?,
                index(totals)?,
                vertices,
                capacity,
            ]),
            Command::Dispatch { groups: [1, 1, 1] },
            super::gpu::indirect_barrier(&[pairs, args, totals]),
        ]);
        Ok(Self {
            pairs,
            args,
            totals,
            entries: input.entries,
            commands,
        })
    }

    /// The binning's dispatches.
    pub(crate) fn commands(&self) -> &[Command] {
        &self.commands
    }
}

/// A non-meshlet caster instance in one level: its world bounding sphere and where it draws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Unit {
    /// Centre (xyz) and radius (w).
    pub(crate) sphere: Vec4,
    /// Its draw (an index into the caster draws).
    pub(crate) draw: u32,
    pub(crate) level: u32,
    /// Its row in the frame's instance table.
    pub(crate) instance: u32,
}

/// One caster draw's units (consecutive) and vertex count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DrawSpan {
    pub(crate) first: u32,
    pub(crate) units: u32,
    pub(crate) vertices: u32,
}

/// One frame's binning of non-meshlet casters, recorded.
#[derive(Debug, Clone)]
pub(crate) struct UnitBins {
    /// The units (the page draw reads their instances).
    pub(crate) units: Buffer,
    /// The pairs, by draw.
    pub(crate) pairs: Buffer,
    /// One `DrawIndirect` record per draw.
    pub(crate) args: Buffer,
    /// Each draw's first pair.
    pub(crate) bases: Buffer,
    /// [pairs wanted, pairs kept].
    pub(crate) totals: Buffer,
    commands: Vec<Command>,
}

impl UnitBins {
    /// Uploads `units` (ordered by draw, then level) and `spans`, and allocates room for
    /// `capacity` pairs; `None` without units.
    pub(crate) fn prepare(
        device: &dyn Device,
        up: &mut Uploads,
        pipelines: BinPipelines,
        (units, spans, capacity): (&[Unit], &[DrawSpan], u32),
        (params, views, table): (u32, u32, u32),
        keep: &mut Vec<Buffer>,
    ) -> Result<Option<Self>, RenderError> {
        if units.is_empty() || spans.is_empty() {
            return Ok(None);
        }
        debug_assert!(
            spans
                .iter()
                .all(|d| d.first + d.units <= units.len() as u32),
            "spans inside the units"
        );
        let count = units.len() as u32;
        let mut rows = Vec::with_capacity(units.len() * 32);
        for u in units {
            push_rows(&mut rows, &[u.sphere]);
            push_rows_u32(&mut rows, &[[u.draw, u.level, u.instance, 0]]);
        }
        let mut span_rows = Vec::with_capacity(spans.len() * 16);
        let span_words: Vec<[u32; 4]> = spans
            .iter()
            .map(|d| [d.first, d.units, d.vertices, 0])
            .collect();
        push_rows_u32(&mut span_rows, &span_words);
        let units_buf = up.buffer(device, &rows, "vsm units")?;
        let spans_buf = up.buffer(device, &span_rows, "vsm unit draws")?;
        let draws = spans.len() as u32;
        let counts = storage(device, up, u64::from(count) * 4, "vsm unit counts")?;
        let scanned = storage(device, up, u64::from(count + 1) * 4, "vsm unit scan")?;
        let pairs = storage(
            device,
            up,
            u64::from(capacity.max(1)) * PAIR_BYTES,
            "vsm unit pairs",
        )?;
        let args = storage(device, up, u64::from(draws) * 16, "vsm unit arguments")?;
        let bases = storage(device, up, u64::from(draws) * 4, "vsm unit bases")?;
        let totals = storage(device, up, 8, "vsm unit totals")?;
        keep.extend([
            units_buf, spans_buf, counts, scanned, pairs, args, bases, totals,
        ]);
        let scan = Scan::prepare(device, up, pipelines.scan, (counts, scanned), count, keep)?;
        let index = |b: Buffer| device.buffer_index(b).map(|i| i.0);
        let unit_push = words(&[
            index(units_buf)?,
            count,
            params,
            views,
            table,
            index(counts)?,
            index(scanned)?,
            index(pairs)?,
            capacity,
        ]);
        let args_push = words(&[
            index(spans_buf)?,
            draws,
            index(scanned)?,
            index(args)?,
            index(bases)?,
            index(totals)?,
            count,
            capacity,
        ]);
        let per_unit = Command::Dispatch {
            groups: group_rows(count),
        };
        let mut commands = vec![
            Command::SetPipeline(pipelines.unit_count),
            unit_push.clone(),
            per_unit.clone(),
            compute_barrier(&[counts]),
        ];
        commands.extend_from_slice(scan.commands());
        commands.extend([
            Command::SetPipeline(pipelines.unit_emit),
            unit_push,
            per_unit,
            Command::SetPipeline(pipelines.unit_args),
            args_push,
            Command::Dispatch {
                groups: [draws.div_ceil(64).max(1), 1, 1],
            },
            super::gpu::indirect_barrier(&[pairs, args, bases, totals]),
        ]);
        Ok(Some(Self {
            units: units_buf,
            pairs,
            args,
            bases,
            totals,
            commands,
        }))
    }

    /// The binning's dispatches; the pairs, arguments and bases are ready for indirect draws and
    /// vertex reads after them.
    pub(crate) fn commands(&self) -> &[Command] {
        &self.commands
    }
}

/// The units of M1 caster draws `casters` (consecutive instances each, whose world spheres are
/// `spheres` in caster order) in `levels` levels, ordered by draw, then level, then instance; and
/// each draw's span with its vertex count.
pub(crate) fn units_for(
    casters: &[crate::shadow_pass::ShadowDraw],
    spheres: &[Vec4],
    levels: u32,
) -> (Vec<Unit>, Vec<DrawSpan>) {
    let (mut units, mut spans) = (Vec::new(), Vec::with_capacity(casters.len()));
    let mut sphere = 0usize;
    for (d, draw) in casters.iter().enumerate() {
        let first = units.len() as u32;
        for level in 0..levels {
            for k in 0..draw.instances {
                units.push(Unit {
                    sphere: spheres
                        .get(sphere + k as usize)
                        .copied()
                        .unwrap_or(Vec4::ZERO),
                    draw: d as u32,
                    level,
                    instance: draw.push.instance + k,
                });
            }
        }
        sphere += draw.instances as usize;
        spans.push(DrawSpan {
            first,
            units: units.len() as u32 - first,
            vertices: draw.vertices,
        });
    }
    debug_assert_eq!(sphere, spheres.len(), "one sphere per caster instance");
    (units, spans)
}

/// The world bounding sphere (centre, radius) of the object box `bounds` placed by `world`.
pub(crate) fn world_sphere(
    bounds: (concordia_core::math::Vec3, concordia_core::math::Vec3),
    world: &concordia_core::math::Mat4,
) -> Vec4 {
    let (lo, hi) = crate::shadow_pass::world_bounds(std::iter::once((bounds, *world)));
    ((lo + hi) * 0.5).extend((hi - lo).length() * 0.5)
}

/// The sphere VSM invalidation uses for a caster with object-space box `bounds` placed by `world`
/// (M4 4.7b): it holds every sphere binning can draw it with (the box's world sphere, and each
/// meshlet's sphere inside the box, scaled by the largest column as `max_scale` does).
pub fn caster_sphere(
    bounds: (concordia_core::math::Vec3, concordia_core::math::Vec3),
    world: &concordia_core::math::Mat4,
) -> Vec4 {
    let boxed = world_sphere(bounds, world);
    let (lo, hi) = bounds;
    let max_scale = [world.x_axis, world.y_axis, world.z_axis]
        .map(|c| c.truncate().length())
        .into_iter()
        .fold(0.0f32, f32::max);
    let scaled = world
        .transform_point3((lo + hi) * 0.5)
        .extend((hi - lo).length() * 0.5 * max_scale);
    let s = enclose(boxed, scaled);
    s.truncate().extend(s.w * (1.0 + 1e-4) + 1e-4) // rounding margin
}

/// The smallest sphere holding spheres `a` and `b` (centre, radius).
fn enclose(a: Vec4, b: Vec4) -> Vec4 {
    let d = b.truncate() - a.truncate();
    let distance = d.length();
    if distance + b.w <= a.w {
        return a;
    }
    if distance + a.w <= b.w {
        return b;
    }
    let radius = (distance + a.w + b.w) * 0.5;
    (a.truncate() + d / distance * (radius - a.w)).extend(radius)
}

/// One group per item for `n` items, in rows of 65 535 groups.
fn group_rows(n: u32) -> [u32; 3] {
    let n = n.max(1);
    match n <= 65_535 {
        true => [n, 1, 1],
        false => [65_535, n.div_ceil(65_535), 1],
    }
}

/// Push constants of `words`.
fn words(w: &[u32]) -> Command {
    Command::PushConstants(w.iter().flat_map(|w| w.to_le_bytes()).collect())
}

/// A frame's GPU-only storage buffer of `bytes` bytes (indirect arguments too).
pub(crate) fn storage(
    device: &dyn Device,
    up: &Uploads,
    bytes: u64,
    label: &str,
) -> Result<Buffer, RenderError> {
    Ok(up.alloc(
        device,
        &BufferDesc {
            size: NonZeroU64::new(bytes).unwrap_or(NonZeroU64::MIN),
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use concordia_core::math::{IVec2, Vec3};

    use super::*;
    use crate::test_gpu::{self, download, run_commands};
    use crate::vsm::frame::{Footprint, ParamsIn, depth_views, params_rows};
    use crate::vsm::{Clipmap, PageEntry, PageFlags, VsmSettings, table_slot};

    fn pipelines(device: &dyn Device) -> BinPipelines {
        let k = |entry| test_gpu::compute(device, "vsm_page.slang", entry);
        BinPipelines {
            unit_count: k("vsm_unit_count"),
            unit_emit: k("vsm_unit_emit"),
            unit_args: k("vsm_unit_args"),
            meshlet_count: test_gpu::compute(device, "meshlet.slang", "meshlet_vsm_count"),
            meshlet_emit: test_gpu::compute(device, "meshlet.slang", "meshlet_vsm_emit"),
            meshlet_args: test_gpu::compute(device, "meshlet.slang", "meshlet_vsm_args"),
            meshlet_ranks: test_gpu::compute(device, "meshlet.slang", "meshlet_vsm_ranks"),
            zero: test_gpu::compute(device, "vsm_page.slang", "vsm_zero"),
            scan: ScanPipelines {
                groups: test_gpu::compute(device, "scan.slang", "scan_groups"),
                add: test_gpu::compute(device, "scan.slang", "scan_add"),
            },
        }
    }

    /// The pages of `level` a CPU walk finds under `sphere` (the shader's rect: centre ± radius in
    /// texels, one texel wider), row by row, that are due to be drawn: (page in level, physical).
    fn dirty_under(clipmap: &Clipmap, table: &[u32], level: u32, sphere: Vec4) -> Vec<(u32, u32)> {
        let l = &clipmap.levels[level as usize];
        crate::vsm::testing::pages_under(clipmap, level, sphere)
            .into_iter()
            .filter_map(|p| {
                let slot = table_slot(level, l.world_page(p).into());
                let e = PageEntry::from_bits(table[slot as usize]);
                match (e.physical(), e.flags().contains(PageFlags::DIRTY)) {
                    (Some(phys), true) => Some(((p.y * 128 + p.x) as u32, u32::from(phys))),
                    _ => None,
                }
            })
            .collect()
    }

    /// Every unit gets one pair per page due to be drawn under its sphere, in unit order and row by
    /// row; pages without a physical page or not due are skipped; each draw's arguments count its
    /// pairs and its base is where they start; pairs past the capacity are dropped and counted.
    /// Under rotations and non-uniform scales, the invalidation sphere holds the box's world sphere
    /// and every sphere inside the box as binning places it (centre transformed, radius times the
    /// largest column).
    #[test]
    fn caster_sphere_holds_every_binning_sphere() {
        use concordia_core::math::{Mat4, Quat};
        let bounds = (Vec3::new(-1.0, -0.5, -2.0), Vec3::new(3.0, 0.5, 1.0));
        let inner = [
            (Vec3::new(-0.4, 0.0, -1.4), 0.5),
            (Vec3::new(2.5, 0.0, 0.5), 0.5),
            (Vec3::new(1.0, 0.0, -0.5), 0.5),
        ];
        let max_scale = |m: &Mat4| {
            [m.x_axis, m.y_axis, m.z_axis]
                .map(|c| c.truncate().length())
                .into_iter()
                .fold(0.0f32, f32::max)
        };
        for (i, scale) in [
            Vec3::ONE,
            Vec3::new(4.0, 0.25, 1.0),
            Vec3::new(0.3, 2.0, 7.0),
        ]
        .into_iter()
        .enumerate()
        {
            let rotation = Quat::from_euler(
                concordia_core::math::EulerRot::XYZ,
                0.3 * i as f32,
                1.1,
                -0.7,
            );
            let world =
                Mat4::from_scale_rotation_translation(scale, rotation, Vec3::new(5.0, -2.0, 9.0));
            let s = caster_sphere(bounds, &world);
            let holds = |c: Vec3, r: f32| c.distance(s.truncate()) + r <= s.w * (1.0 + 1e-5) + 1e-5;
            let b = world_sphere(bounds, &world);
            assert!(holds(b.truncate(), b.w), "box sphere, scale {scale}");
            for (c, r) in inner {
                let wc = world.transform_point3(c);
                assert!(
                    holds(wc, r * max_scale(&world)),
                    "inner sphere, scale {scale}"
                );
            }
        }
    }

    #[test]
    fn binning_pairs_each_caster_with_its_dirty_pages() {
        let Some(device) = test_gpu::device() else {
            return;
        };
        let device = device.as_ref();
        let settings = VsmSettings {
            levels: 4,
            ..VsmSettings::default()
        };
        let eye = Vec3::new(1.0, 3.0, 2.0);
        let clipmap = Clipmap::fit(Vec3::new(-0.3, -1.0, 0.2), eye, 4, (-50.0, 50.0));
        // Pages due around the eye in every level; some only requested (no page), some kept.
        let mut table = vec![0u32; 4 * 128 * 128];
        let mut physical = 0u16;
        for level in 0..4u32 {
            let l = &clipmap.levels[level as usize];
            for y in 56..72 {
                for x in 56..72 {
                    let slot = table_slot(level, l.world_page(IVec2::new(x, y)).into());
                    let entry = match (x + y) % 5 {
                        0 => PageEntry::new(None, PageFlags::REQUESTED, 0),
                        1 => PageEntry::new(Some(physical), PageFlags::VALID, 0),
                        _ => PageEntry::new(Some(physical), PageFlags::VALID | PageFlags::DIRTY, 0),
                    };
                    physical += 1;
                    table[slot as usize] = entry.bits();
                }
            }
        }
        let spheres = [
            Vec4::new(1.0, 0.0, 2.0, 0.3),
            Vec4::new(1.5, 0.5, 1.0, 2.0),
            Vec4::new(-30.0, 0.0, 40.0, 1.0),
        ];
        let mut units = Vec::new();
        let mut spans = Vec::new();
        for (draw, instances) in [(0u32, 0..2u32), (1, 2..3)] {
            let first = units.len() as u32;
            for level in 0..4 {
                for instance in instances.clone() {
                    units.push(Unit {
                        sphere: spheres[instance as usize],
                        draw,
                        level,
                        instance,
                    });
                }
            }
            spans.push(DrawSpan {
                first,
                units: units.len() as u32 - first,
                vertices: 36 + 3 * draw,
            });
        }
        let mut want: BTreeMap<u32, Vec<[u32; 4]>> = BTreeMap::new();
        for (i, u) in units.iter().enumerate() {
            for (page, phys) in dirty_under(&clipmap, &table, u.level, u.sphere) {
                want.entry(u.draw)
                    .or_default()
                    .push([i as u32, u.level, page, phys]);
            }
        }
        let wanted: usize = want.values().map(Vec::len).sum();
        assert!(wanted > 20, "the scene pairs something: {wanted}");
        let origins: Vec<IVec2> = clipmap.levels.iter().map(|l| l.origin_page()).collect();
        let run = |capacity: u32| {
            let mut up = Uploads::new();
            let mut keep = Vec::new();
            let params = params_rows(&ParamsIn {
                settings: &settings,
                clipmap: &clipmap,
                eye,
                footprint: Footprint::PerMetre(0.001),
                pcf_radius: 1,
                chunks: &[],
                inv_view_proj: Default::default(),
                previous: &origins,
            });
            let params = up.buffer(device, &params, "params").unwrap();
            let views = up
                .buffer(device, &depth_views(&Default::default(), &clipmap), "views")
                .unwrap();
            let bytes: Vec<u8> = table.iter().flat_map(|w| w.to_le_bytes()).collect();
            let table = up.buffer(device, &bytes, "table").unwrap();
            let index = |b: Buffer| device.buffer_index(b).unwrap().0;
            let bins = UnitBins::prepare(
                device,
                &mut up,
                pipelines(device),
                (&units, &spans, capacity),
                (index(params), index(views), index(table)),
                &mut keep,
            )
            .unwrap()
            .expect("units");
            up.flush(device).unwrap();
            run_commands(device, bins.commands().to_vec());
            (
                download(device, bins.pairs, u64::from(capacity) * PAIR_BYTES),
                download(device, bins.args, 2 * 16),
                download(device, bins.bases, 2 * 4),
                download(device, bins.totals, 2 * 4),
            )
        };
        let (pairs, args, bases, totals) = run(4096);
        assert_eq!(totals, [wanted as u32, wanted as u32]);
        for (draw, span) in spans.iter().enumerate() {
            let w = &want.get(&(draw as u32)).cloned().unwrap_or_default();
            let base = bases[draw] as usize;
            let got: Vec<[u32; 4]> = (0..w.len())
                .map(|k| {
                    let at = (base + k) * 4;
                    [pairs[at], pairs[at + 1], pairs[at + 2], pairs[at + 3]]
                })
                .collect();
            assert_eq!(&got, w, "draw {draw}");
            assert_eq!(
                &args[draw * 4..draw * 4 + 4],
                [span.vertices, w.len() as u32, 0, 0]
            );
        }
        let small = (wanted / 2) as u32;
        let (_, args, _, totals) = run(small);
        assert_eq!(totals, [wanted as u32, small], "dropped pairs are counted");
        assert!(args[1] + args[5] <= small, "never drawn past the capacity");
    }
}
