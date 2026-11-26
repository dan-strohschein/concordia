//! The meshlet path's CPU side (ADR-0100, M4 4.3a): which draws take it, the mesh table, and the
//! buckets and work items that `meshlet.cull` culls and packs into indirect draws (4.4).

use concordia_cluster::{ClusterError, MeshletMesh};
use concordia_core::math::Vec3;

/// What the GPU needs of a mesh's `MeshletBlob` v1 (decoded and checked once, at upload).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct MeshletInfo {
    /// Meshlets in the blob.
    pub(crate) meshlets: u32,
    /// The build's triangles per meshlet (64 or 128).
    pub(crate) max_triangles: u32,
}

impl MeshletInfo {
    /// The blob's counts; `None` for a mesh without meshlets, an error when the blob is invalid.
    pub(crate) fn of(blob: &[u8]) -> Result<Option<Self>, ClusterError> {
        if blob.is_empty() {
            return Ok(None);
        }
        let mesh = MeshletMesh::decode(blob)?;
        let meshlets = u32::try_from(mesh.meshlets.len())
            .map_err(|_| ClusterError::Input("more than 2^32 meshlets".into()))?;
        Ok(Some(Self {
            meshlets,
            max_triangles: mesh.settings.size().triangles(),
        }))
    }
}

/// Whether a draw takes the meshlet path: the path is on, the mesh has meshlets, and the draw is
/// neither skinned nor blended (those stay on the M1 path).
pub(crate) fn takes_path(
    enabled: bool,
    meshlets: Option<MeshletInfo>,
    skinned: bool,
    blend: bool,
) -> bool {
    enabled && !skinned && !blend && meshlets.is_some_and(|m| m.meshlets > 0)
}

/// How meshlet buckets reach the rasterizer this frame (ADR-0100).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Route {
    /// `meshlet.cull` then one indirect draw per bucket (`meshlet_vs`).
    ComputeIndirect,
    /// One mesh-task draw per bucket (`meshlet_object` and `meshlet_mesh`).
    MeshTasks,
}

/// Vertices and triangles the mesh stage declares per group (`meshlet.slang`'s `meshlet_mesh`).
pub(crate) const MESH_OUTPUTS: u32 = 128;
/// Entries one object group takes (`meshlet_object`'s threads and payload).
pub(crate) const OBJECT_ENTRIES: u32 = 32;
/// Object groups per row of a mesh-task draw (the most a dimension allows everywhere).
const TASK_ROW: u32 = concordia_rhi::MAX_MESH_TASK_GROUPS;

/// The mesh path when the device has object and mesh shaders that take [`MESH_OUTPUTS`], and the
/// view needs no per-triangle data (`wireframe` and `triangles` stay on the compute path: shared
/// mesh-stage vertices can't carry it).
pub(crate) fn route(caps: &concordia_rhi::RhiCaps, view_mode: crate::ViewMode) -> Route {
    let mesh = caps.mesh_shaders
        && caps.object_shaders
        && caps.max_mesh_output_vertices >= MESH_OUTPUTS
        && caps.max_mesh_output_primitives >= MESH_OUTPUTS;
    let per_triangle = matches!(
        view_mode,
        crate::ViewMode::wireframe | crate::ViewMode::triangles
    );
    match mesh && !per_triangle {
        true => Route::MeshTasks,
        false => Route::ComputeIndirect,
    }
}

/// The object groups of a mesh-task draw covering `entries` entries, [`OBJECT_ENTRIES`] each, in
/// rows of at most [`TASK_ROW`] groups (`meshlet_object` numbers them `y * TASK_ROW + x`).
pub(crate) fn task_groups(entries: u32) -> [u32; 3] {
    let groups = entries.div_ceil(OBJECT_ENTRIES);
    match groups <= TASK_ROW {
        true => [groups, 1, 1],
        false => [TASK_ROW, groups.div_ceil(TASK_ROW), 1],
    }
}

/// Bindless indices of a mesh's vertex streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Streams {
    pub(crate) positions: u32,
    pub(crate) normals: u32,
    pub(crate) tangents: u32,
    pub(crate) uvs: u32,
}

/// A mesh-table row (four `uint4`, mirrored by `meshlet.slang`'s `MESH_ROWS`): blob, positions,
/// normals, tangents; UVs, meshlet count, triangles per meshlet, 0; the local box's lo and hi
/// (float bits, for instance culling).
pub(crate) fn mesh_row(
    blob: u32,
    streams: Streams,
    info: MeshletInfo,
    (lo, hi): (Vec3, Vec3),
) -> [[u32; 4]; 4] {
    let bits = |v: Vec3| [v.x.to_bits(), v.y.to_bits(), v.z.to_bits(), 0];
    [
        [blob, streams.positions, streams.normals, streams.tangents],
        [streams.uvs, info.meshlets, info.max_triangles, 0],
        bits(lo),
        bits(hi),
    ]
}

/// Push constants of `meshlet_vs` and `meshlet_shadow_vs` (mirrors `meshlet.slang`); `frame`,
/// `materials` and `material` sit where `DrawPush` has them, for `fs`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MeshletPush {
    /// Frame block buffer.
    pub frame: u32,
    /// Instance buffer.
    pub instances: u32,
    /// Entry buffer (instance, meshlet).
    pub entries: u32,
    /// Material buffer.
    pub materials: u32,
    /// The bucket's material's first row.
    pub material: u32,
    /// Mesh table buffer.
    pub meshes: u32,
    /// The bucket's first entry.
    pub first: u32,
    /// Shadow cascade (shadow draws only).
    pub cascade: u32,
}

concordia_shaders::shader_struct!(MeshletPush {
    frame,
    instances,
    entries,
    materials,
    material,
    meshes,
    first,
    cascade
});

impl MeshletPush {
    /// The block's words in layout order (the cascade last).
    pub(crate) fn words(&self) -> [u32; 8] {
        [
            self.frame,
            self.instances,
            self.entries,
            self.materials,
            self.material,
            self.meshes,
            self.first,
            self.cascade,
        ]
    }

    /// Little-endian bytes for `Command::PushConstants`.
    pub fn bytes(&self) -> Vec<u8> {
        words(self.words())
    }
}

/// Push constants of the mesh-shader bucket stages (mirrors `meshlet.slang`'s `TaskPush`);
/// `frame`, `materials` and `material` sit where `DrawPush` has them, for `fs`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TaskPush {
    /// Frame block buffer.
    pub frame: u32,
    /// Instance buffer.
    pub instances: u32,
    /// Work item buffer (instance, mesh row, first entry, bucket).
    pub items: u32,
    /// Material buffer.
    pub materials: u32,
    /// The bucket's material's first row.
    pub material: u32,
    /// Mesh table buffer.
    pub meshes: u32,
    /// The bucket's first entry.
    pub first: u32,
    /// The bucket's entries.
    pub count: u32,
    /// Work items in the buffer.
    pub item_count: u32,
    /// Cull view table buffer.
    pub views: u32,
    /// Cull code buffer (one word per entry).
    pub codes: u32,
    /// The bucket's cull word (view, no-cone flag).
    pub cull: u32,
    /// Shadow cascade (shadow draws only).
    pub cascade: u32,
    /// Which culling pass draws (`meshlet::PASS_*`).
    pub pass: u32,
    /// Visibility buffer (one word per meshlet of each instance; two-pass occlusion).
    pub visibility: u32,
}

concordia_shaders::shader_struct!(TaskPush {
    frame,
    instances,
    items,
    materials,
    material,
    meshes,
    first,
    count,
    item_count,
    views,
    codes,
    cull,
    cascade,
    pass,
    visibility
});

impl TaskPush {
    /// The block's words in layout order.
    pub(crate) fn words(&self) -> [u32; 15] {
        [
            self.frame,
            self.instances,
            self.items,
            self.materials,
            self.material,
            self.meshes,
            self.first,
            self.count,
            self.item_count,
            self.views,
            self.codes,
            self.cull,
            self.cascade,
            self.pass,
            self.visibility,
        ]
    }

    /// Little-endian bytes for `Command::PushConstants`.
    pub fn bytes(&self) -> Vec<u8> {
        words(self.words())
    }
}

fn words<const N: usize>(values: [u32; N]) -> Vec<u8> {
    values.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// The one culling pass of a frame without two-pass occlusion (mirrors `meshlet.slang`).
pub(crate) const PASS_SINGLE: u32 = 0;
/// Pass 1 of two-pass occlusion: what was visible last frame, without the HZB.
pub(crate) const PASS_EARLY: u32 = 1;
/// Pass 2: every candidate against this frame's HZB, drawing what pass 1 didn't.
pub(crate) const PASS_LATE: u32 = 2;

/// Push constants of `meshlet_cull` (mirrors `meshlet.slang`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CullPush {
    /// Work item buffer.
    pub items: u32,
    /// Work items.
    pub item_count: u32,
    /// Bucket table buffer (first entry, entries, vertices, cull word).
    pub buckets: u32,
    /// Cull view table buffer.
    pub views: u32,
    /// Instance buffer.
    pub instances: u32,
    /// Mesh table buffer.
    pub meshes: u32,
    /// Cull code buffer (one word per entry).
    pub codes: u32,
    /// Drawn flag buffer (one word per entry; the scan's input).
    pub drawn: u32,
    /// Entries in all buckets.
    pub total: u32,
    /// Which culling pass this is (`meshlet::PASS_*`).
    pub pass: u32,
    /// Visibility buffer (one word per meshlet of each instance; two-pass occlusion).
    pub visibility: u32,
}

concordia_shaders::shader_struct!(CullPush {
    items,
    item_count,
    buckets,
    views,
    instances,
    meshes,
    codes,
    drawn,
    total,
    pass,
    visibility
});

impl CullPush {
    /// Little-endian bytes for `Command::PushConstants`.
    pub fn bytes(&self) -> Vec<u8> {
        words([
            self.items,
            self.item_count,
            self.buckets,
            self.views,
            self.instances,
            self.meshes,
            self.codes,
            self.drawn,
            self.total,
            self.pass,
            self.visibility,
        ])
    }
}

/// Push constants of `meshlet_compact` (mirrors `meshlet.slang`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompactPush {
    /// Work item buffer.
    pub items: u32,
    /// Work items.
    pub item_count: u32,
    /// Bucket table buffer.
    pub buckets: u32,
    /// Drawn flag buffer.
    pub drawn: u32,
    /// The drawn flags' exclusive sums (one more than the entries).
    pub sums: u32,
    /// Entry buffer (instance, meshlet), packed per bucket.
    pub entries: u32,
    /// Indirect argument buffer.
    pub args: u32,
    /// Entries in all buckets.
    pub total: u32,
}

concordia_shaders::shader_struct!(CompactPush {
    items,
    item_count,
    buckets,
    drawn,
    sums,
    entries,
    args,
    total
});

impl CompactPush {
    /// Little-endian bytes for `Command::PushConstants`.
    pub fn bytes(&self) -> Vec<u8> {
        words([
            self.items,
            self.item_count,
            self.buckets,
            self.drawn,
            self.sums,
            self.entries,
            self.args,
            self.total,
        ])
    }
}

/// One draw on the meshlet path, in the M1 path's draw order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PathDraw<K> {
    /// Its row in the frame's instance table.
    pub(crate) instance: u32,
    /// Its mesh's row in the mesh table.
    pub(crate) mesh_row: u32,
    pub(crate) info: MeshletInfo,
    /// The forward bucket it draws in (pipeline and material), when the camera sees it.
    pub(crate) forward: Option<K>,
    /// It casts sun shadows.
    pub(crate) casts: bool,
    /// Its material draws back faces (no cone test).
    pub(crate) double_sided: bool,
}

/// One indirect draw: a contiguous range of entries, one instance per meshlet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Bucket<K> {
    /// The forward pipeline and material; `None` for the shadow casters.
    pub(crate) key: Option<K>,
    /// Its first entry.
    pub(crate) first: u32,
    /// Its entries (meshlets).
    pub(crate) entries: u32,
    /// Vertices per instance: 3 × the largest triangles per meshlet among its meshes.
    pub(crate) vertices: u32,
    /// The view it is culled for: 0 the camera, 1 + c shadow cascade c.
    pub(crate) view: u32,
    /// Its meshlets may be cone-culled (single-sided forward buckets).
    pub(crate) cone: bool,
}

/// A bucket's cull word's flag for buckets without the cone test (`meshlet.slang`).
pub(crate) const BUCKET_NO_CONE: u32 = 0x100;

impl<K> Bucket<K> {
    /// The cull word the shaders read: the view, and [`BUCKET_NO_CONE`] when it has no cone test.
    pub(crate) fn cull_word(&self) -> u32 {
        debug_assert!(self.view < 256, "a view index fits the word's low byte");
        match self.cone {
            true => self.view,
            false => self.view | BUCKET_NO_CONE,
        }
    }
}

/// A frame's meshlet work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MeshletPlan<K> {
    /// Work items, by bucket: (instance, mesh row, first entry, bucket).
    pub(crate) items: Vec<[u32; 4]>,
    /// Forward buckets in order of first use, then the shadow bucket if anything casts.
    pub(crate) buckets: Vec<Bucket<K>>,
    /// Entries in all buckets.
    pub(crate) entries: u32,
}

/// Groups `draws` into buckets (forward by key, then the shadow casters once per cascade, for
/// `cascades` cascades) and lays out their entries.
pub(crate) fn plan<K: Copy + Eq>(draws: &[PathDraw<K>], cascades: u32) -> MeshletPlan<K> {
    // (forward key, or the cascade's view for shadows)
    let mut keys: Vec<Result<K, u32>> = Vec::new();
    for key in draws.iter().filter_map(|d| d.forward) {
        if !keys.contains(&Ok(key)) {
            keys.push(Ok(key));
        }
    }
    if draws.iter().any(|d| d.casts) {
        keys.extend((1..=cascades).map(Err));
    }
    let mut out = MeshletPlan {
        items: Vec::new(),
        buckets: Vec::with_capacity(keys.len()),
        entries: 0,
    };
    for (index, key) in keys.into_iter().enumerate() {
        let members: Vec<&PathDraw<K>> = draws
            .iter()
            .filter(|d| match key {
                Ok(k) => d.forward == Some(k),
                Err(_) => d.casts,
            })
            .collect();
        let mut b = bucket(
            &mut out.items,
            &mut out.entries,
            index as u32,
            key.ok(),
            members.iter().copied(),
        );
        b.view = key.err().unwrap_or(0);
        // Members of a forward bucket share its pipeline, so they agree on double-sidedness;
        // cascades test no cone (shadow pipelines cull no faces).
        b.cone = key.is_ok() && !members.iter().any(|d| d.double_sided);
        out.buckets.push(b);
    }
    out
}

/// Lays out one bucket's members from entry `*next`, appending their work items.
fn bucket<'a, K: Copy + 'a>(
    items: &mut Vec<[u32; 4]>,
    next: &mut u32,
    index: u32,
    key: Option<K>,
    members: impl Iterator<Item = &'a PathDraw<K>>,
) -> Bucket<K> {
    let first = *next;
    let mut max_triangles = 0;
    for d in members {
        items.push([d.instance, d.mesh_row, *next, index]);
        *next += d.info.meshlets;
        max_triangles = max_triangles.max(d.info.max_triangles);
    }
    Bucket {
        key,
        first,
        entries: *next - first,
        vertices: 3 * max_triangles,
        view: 0,
        cone: true,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use concordia_cluster::{ClusterInput, MeshletBuildSettings, MeshletSize};
    use concordia_core::jobs::JobSystem;
    use proptest::prelude::*;

    const INFO: MeshletInfo = MeshletInfo {
        meshlets: 3,
        max_triangles: 128,
    };

    /// A grid of `n`×`n` quads built into meshlets.
    pub(crate) fn grid_blob(n: u32) -> Vec<u8> {
        let positions: Vec<[f32; 3]> = (0..=n)
            .flat_map(|y| (0..=n).map(move |x| [x as f32, y as f32, 0.0]))
            .collect();
        let at = |x: u32, y: u32| y * (n + 1) + x;
        let indices: Vec<u32> = (0..n)
            .flat_map(|y| (0..n).map(move |x| (x, y)))
            .flat_map(|(x, y)| {
                [
                    at(x, y),
                    at(x + 1, y),
                    at(x + 1, y + 1),
                    at(x, y),
                    at(x + 1, y + 1),
                    at(x, y + 1),
                ]
            })
            .collect();
        let input = ClusterInput::new(&positions, &indices, None).unwrap();
        let settings = MeshletBuildSettings::for_size(MeshletSize::S128);
        let jobs = JobSystem::new(std::num::NonZeroUsize::MIN).unwrap();
        concordia_cluster::build(&input, &settings, &jobs)
            .unwrap()
            .encode()
            .unwrap()
    }

    #[test]
    fn blobs_give_their_counts_and_bad_blobs_are_errors() {
        let bytes = grid_blob(16);
        let mesh = MeshletMesh::decode(&bytes).unwrap();
        assert_eq!(
            MeshletInfo::of(&bytes).unwrap(),
            Some(MeshletInfo {
                meshlets: mesh.meshlets.len() as u32,
                max_triangles: 128,
            })
        );
        assert!(mesh.meshlets.len() > 1, "a 512-triangle grid has several");
        assert_eq!(MeshletInfo::of(&[]).unwrap(), None, "no meshlets");
        let mut corrupt = bytes.clone();
        corrupt[0] = b'X';
        assert!(MeshletInfo::of(&corrupt).is_err(), "bad magic");
        assert!(
            MeshletInfo::of(&bytes[..bytes.len() - 1]).is_err(),
            "truncated"
        );
    }

    #[test]
    fn only_static_unblended_meshes_with_blobs_take_the_meshlet_path() {
        assert!(takes_path(true, Some(INFO), false, false));
        assert!(!takes_path(false, Some(INFO), false, false), "path off");
        assert!(!takes_path(true, None, false, false), "no meshlets");
        assert!(!takes_path(true, Some(INFO), true, false), "skinned");
        assert!(!takes_path(true, Some(INFO), false, true), "blended");
        let empty = MeshletInfo {
            meshlets: 0,
            max_triangles: 128,
        };
        assert!(
            !takes_path(true, Some(empty), false, false),
            "an empty blob"
        );
    }

    #[test]
    fn mesh_table_rows_carry_the_blob_streams_and_local_bounds() {
        let streams = Streams {
            positions: 11,
            normals: 12,
            tangents: 13,
            uvs: 14,
        };
        let bounds = (Vec3::new(-1.0, -2.0, -3.0), Vec3::new(4.0, 5.0, 6.0));
        let f = |v: f32| v.to_bits();
        assert_eq!(
            mesh_row(10, streams, INFO, bounds),
            [
                [10, 11, 12, 13],
                [14, 3, 128, 0],
                [f(-1.0), f(-2.0), f(-3.0), 0],
                [f(4.0), f(5.0), f(6.0), 0]
            ]
        );
    }

    fn draw(instance: u32, meshlets: u32, forward: Option<u32>, casts: bool) -> PathDraw<u32> {
        PathDraw {
            instance,
            mesh_row: instance + 100,
            info: MeshletInfo {
                meshlets,
                max_triangles: 128,
            },
            forward,
            casts,
            double_sided: false,
        }
    }

    #[test]
    fn buckets_follow_m1_batching() {
        // In M1 order: key 7 twice, key 3, key 7 again (a later instance of the same batch).
        let draws = [
            draw(0, 2, Some(7), true),
            draw(1, 3, Some(7), false),
            draw(2, 1, Some(3), true),
            draw(3, 4, Some(7), true),
            draw(4, 5, None, true),
        ];
        let p = plan(&draws, 1);
        let keys: Vec<_> = p.buckets.iter().map(|b| b.key).collect();
        assert_eq!(
            keys,
            [Some(7), Some(3), None],
            "first-use order, shadows last"
        );
        assert_eq!(
            p.buckets[0],
            Bucket {
                key: Some(7),
                first: 0,
                entries: 9,
                vertices: 384,
                view: 0,
                cone: true,
            }
        );
        assert_eq!(
            p.buckets[1],
            Bucket {
                key: Some(3),
                first: 9,
                entries: 1,
                vertices: 384,
                view: 0,
                cone: true,
            }
        );
        assert_eq!(
            p.buckets[2],
            Bucket {
                key: None,
                first: 10,
                entries: 12,
                vertices: 384,
                view: 1,
                cone: false,
            }
        );
        assert_eq!(p.entries, 22);
        assert_eq!(
            p.items[..3],
            [[0, 100, 0, 0], [1, 101, 2, 0], [3, 103, 5, 0]],
            "bucket 0's items in draw order"
        );
        assert_eq!(p.items[3], [2, 102, 9, 1]);
        assert_eq!(
            p.items[4..],
            [
                [0, 100, 10, 2],
                [2, 102, 12, 2],
                [3, 103, 13, 2],
                [4, 104, 17, 2]
            ]
        );
    }

    #[test]
    fn plan_lays_out_one_shadow_region_per_cascade() {
        let draws = [draw(0, 2, Some(7), true), draw(1, 3, None, true)];
        let p = plan(&draws, 3);
        let regions: Vec<_> = p
            .buckets
            .iter()
            .map(|b| (b.key, b.view, b.first, b.entries))
            .collect();
        assert_eq!(
            regions,
            [
                (Some(7), 0, 0, 2),
                (None, 1, 2, 5),
                (None, 2, 7, 5),
                (None, 3, 12, 5)
            ]
        );
        assert_eq!(p.entries, 17);
        assert!(
            plan(&draws, 0).buckets.iter().all(|b| b.key.is_some()),
            "no cascades, no casters"
        );
    }

    #[test]
    fn double_sided_buckets_skip_the_cone_test() {
        let mut two = draw(0, 2, Some(7), false);
        two.double_sided = true;
        let p = plan(&[two, draw(1, 1, Some(8), false)], 0);
        assert_eq!(
            p.buckets.iter().map(|b| b.cone).collect::<Vec<_>>(),
            [false, true]
        );
        assert_eq!(p.buckets[0].cull_word(), 0x100);
        let shadow = plan(&[draw(0, 1, None, true)], 2);
        assert_eq!(
            shadow.buckets[1].cull_word(),
            2 | BUCKET_NO_CONE,
            "cascade views by index, no cone"
        );
    }

    #[test]
    fn vertices_cover_the_largest_meshlets_in_a_bucket() {
        let mut small = draw(1, 2, Some(1), false);
        small.info.max_triangles = 64;
        let p = plan(&[small], 0);
        assert_eq!(p.buckets[0].vertices, 192);
        let p = plan(&[small, draw(0, 1, Some(1), false)], 0);
        assert_eq!(p.buckets[0].vertices, 384);
    }

    #[test]
    fn nothing_to_draw_plans_nothing() {
        let p = plan(&[draw(0, 2, None, false)], 1);
        assert_eq!((p.items.len(), p.buckets.len(), p.entries), (0, 0, 0));
    }

    fn caps() -> concordia_rhi::RhiCaps {
        concordia_rhi::RhiCaps {
            backend: concordia_rhi::Backend::Metal,
            device_name: "test".into(),
            unified_memory: true,
            msaa_sample_counts: vec![concordia_rhi::SampleCount::X4],
            bindless: true,
            mesh_shaders: true,
            object_shaders: true,
            ray_tracing: false,
            hw_ray_tracing: false,
            metalfx_temporal: false,
            depth_resolve_min: false,
            timestamp_granularity: concordia_rhi::TimestampGranularity::StageBoundary,
            max_mesh_output_vertices: 256,
            max_mesh_output_primitives: 512,
            max_buffer_bytes: 1 << 30,
            max_bindless_textures: 16_384,
            max_bindless_buffers: 16_384,
            feature_sets: vec![],
        }
    }

    #[test]
    fn path_selection_follows_caps_and_view_mode() {
        use crate::ViewMode;
        assert_eq!(route(&caps(), ViewMode::lit), Route::MeshTasks);
        assert_eq!(route(&caps(), ViewMode::overdraw), Route::MeshTasks);
        assert_eq!(route(&caps(), ViewMode::meshlets), Route::MeshTasks);
        let masked = caps().masked(&concordia_rhi::CapsMask {
            mesh_shaders: true,
            ..Default::default()
        });
        assert_eq!(route(&masked, ViewMode::lit), Route::ComputeIndirect);
        let mut no_object = caps();
        no_object.object_shaders = false;
        assert_eq!(route(&no_object, ViewMode::lit), Route::ComputeIndirect);
        for view in [ViewMode::wireframe, ViewMode::triangles] {
            assert_eq!(route(&caps(), view), Route::ComputeIndirect, "{view:?}");
        }
    }

    #[test]
    fn devices_below_the_mesh_stages_outputs_take_the_compute_path() {
        let mut small = caps();
        small.max_mesh_output_vertices = MESH_OUTPUTS - 1;
        assert_eq!(route(&small, crate::ViewMode::lit), Route::ComputeIndirect);
        let mut small = caps();
        small.max_mesh_output_primitives = MESH_OUTPUTS - 1;
        assert_eq!(route(&small, crate::ViewMode::lit), Route::ComputeIndirect);
        let mut exact = caps();
        exact.max_mesh_output_vertices = MESH_OUTPUTS;
        exact.max_mesh_output_primitives = MESH_OUTPUTS;
        assert_eq!(route(&exact, crate::ViewMode::lit), Route::MeshTasks);
    }

    /// The entry slot `meshlet_object`'s lane `lane` of group (x, y) takes, if any (mirrors the
    /// shader).
    fn task_slot(x: u32, y: u32, lane: u32, entries: u32) -> Option<u32> {
        let slot = (y * TASK_ROW + x) * OBJECT_ENTRIES + lane;
        (slot < entries).then_some(slot)
    }

    #[test]
    fn task_groups_fill_rows_first() {
        assert_eq!(task_groups(1), [1, 1, 1]);
        assert_eq!(task_groups(32), [1, 1, 1]);
        assert_eq!(task_groups(33), [2, 1, 1]);
        let row = TASK_ROW * OBJECT_ENTRIES;
        assert_eq!(task_groups(row), [TASK_ROW, 1, 1]);
        assert_eq!(task_groups(row + 1), [TASK_ROW, 2, 1]);
    }

    proptest! {
        #[test]
        fn object_groups_cover_every_entry_once(entries in 1u32..200_000, wide in any::<bool>()) {
            // `wide` jumps past one row of groups (2,097,120 entries) without iterating it all.
            let entries = if wide { TASK_ROW * OBJECT_ENTRIES + entries } else { entries };
            let [x, y, z] = task_groups(entries);
            prop_assert_eq!(z, 1);
            prop_assert!(x <= TASK_ROW && (y == 1 || x == TASK_ROW));
            let groups = u64::from(x) * u64::from(y);
            let needed = u64::from(entries.div_ceil(OBJECT_ENTRIES));
            prop_assert!(groups <= concordia_rhi::MAX_MESH_TASK_GROUPS_TOTAL);
            prop_assert!(groups >= needed, "every entry has a group");
            prop_assert!(u64::from(y - 1) * u64::from(TASK_ROW) < needed, "no empty row");
            prop_assert!(y > 1 || u64::from(x) == needed, "one row: exactly the groups needed");
            // Slots are consecutive over the groups: the last needed group takes the last entry,
            // and a group past it (in a partial last row) takes none.
            let g = needed - 1;
            let (gx, gy) = ((g % u64::from(TASK_ROW)) as u32, (g / u64::from(TASK_ROW)) as u32);
            prop_assert_eq!(task_slot(gx, gy, (entries - 1) % OBJECT_ENTRIES, entries), Some(entries - 1));
            if groups > needed {
                let past = needed;
                let (px, py) = ((past % u64::from(TASK_ROW)) as u32, (past / u64::from(TASK_ROW)) as u32);
                prop_assert_eq!(task_slot(px, py, 0, entries), None);
            }
        }

        #[test]
        fn work_items_cover_every_meshlet_once(
            raw in prop::collection::vec((1u32..40, prop::option::of(0u32..4), any::<bool>()), 0..40),
            cascades in 0u32..4,
        ) {
            let draws: Vec<_> = raw.iter().enumerate()
                .map(|(i, (m, k, c))| draw(i as u32, *m, *k, *c))
                .collect();
            let p = plan(&draws, cascades);
            // Expand every item into (bucket, instance, meshlet) and every bucket's range.
            let mut seen = std::collections::BTreeMap::new();
            for item in &p.items {
                let [instance, row, first, bucket] = *item;
                let d = draws[instance as usize];
                prop_assert_eq!(row, d.mesh_row);
                let b = p.buckets[bucket as usize];
                prop_assert!(first >= b.first && first + d.info.meshlets <= b.first + b.entries);
                for m in 0..d.info.meshlets {
                    prop_assert!(seen.insert(first + m, (bucket, instance, m)).is_none(), "entry used twice");
                }
            }
            prop_assert_eq!(seen.len() as u32, p.entries, "entries are dense");
            prop_assert!(seen.keys().copied().eq(0..p.entries));
            for (b, bucket) in p.buckets.iter().enumerate() {
                let want: u32 = draws.iter()
                    .filter(|d| match bucket.key { Some(k) => d.forward == Some(k), None => d.casts && cascades > 0 })
                    .map(|d| d.info.meshlets)
                    .sum();
                prop_assert_eq!(bucket.entries, want, "bucket {}", b);
            }
            let forward: u32 = draws.iter().filter(|d| d.forward.is_some()).map(|d| d.info.meshlets).sum();
            let shadow: u32 = draws.iter().filter(|d| d.casts).map(|d| d.info.meshlets).sum();
            prop_assert_eq!(p.entries, forward + shadow * cascades);
        }
    }
}
