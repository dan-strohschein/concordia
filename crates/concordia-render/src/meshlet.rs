//! The meshlet path's CPU side (ADR-0100, M4 4.3a): which draws take it, the mesh table, and the
//! buckets and work items that `meshlet.expand` turns into indirect draws.

use concordia_cluster::{ClusterError, MeshletMesh};

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

/// Bindless indices of a mesh's vertex streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Streams {
    pub(crate) positions: u32,
    pub(crate) normals: u32,
    pub(crate) tangents: u32,
    pub(crate) uvs: u32,
}

/// A mesh-table row (two `uint4`, mirrored by `meshlet.slang`): blob, positions, normals,
/// tangents; UVs, meshlet count, triangles per meshlet, 0.
pub(crate) fn mesh_row(blob: u32, streams: Streams, info: MeshletInfo) -> [[u32; 4]; 2] {
    [
        [blob, streams.positions, streams.normals, streams.tangents],
        [streams.uvs, info.meshlets, info.max_triangles, 0],
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
    /// Little-endian bytes for `Command::PushConstants`.
    pub fn bytes(&self) -> Vec<u8> {
        words([
            self.frame,
            self.instances,
            self.entries,
            self.materials,
            self.material,
            self.meshes,
            self.first,
            self.cascade,
        ])
    }
}

impl ExpandPush {
    /// Little-endian bytes for `Command::PushConstants`.
    pub fn bytes(&self) -> Vec<u8> {
        words([
            self.items,
            self.item_count,
            self.buckets,
            self.entries,
            self.args,
            self.total,
        ])
    }
}

fn words<const N: usize>(values: [u32; N]) -> Vec<u8> {
    values.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// Push constants of `expand` (mirrors `meshlet.slang`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExpandPush {
    /// Work item buffer.
    pub items: u32,
    /// Work items.
    pub item_count: u32,
    /// Bucket table buffer (first entry, entries, vertices, 0).
    pub buckets: u32,
    /// Entry buffer.
    pub entries: u32,
    /// Indirect argument buffer.
    pub args: u32,
    /// Entries in all buckets.
    pub total: u32,
}

concordia_shaders::shader_struct!(ExpandPush {
    items,
    item_count,
    buckets,
    entries,
    args,
    total
});

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

/// Groups `draws` into buckets (forward by key, then shadow casters) and lays out their entries.
pub(crate) fn plan<K: Copy + Eq>(draws: &[PathDraw<K>]) -> MeshletPlan<K> {
    let mut keys: Vec<Option<K>> = Vec::new();
    for key in draws.iter().filter_map(|d| d.forward) {
        if !keys.contains(&Some(key)) {
            keys.push(Some(key));
        }
    }
    if draws.iter().any(|d| d.casts) {
        keys.push(None);
    }
    let mut out = MeshletPlan {
        items: Vec::new(),
        buckets: Vec::with_capacity(keys.len()),
        entries: 0,
    };
    for (index, key) in keys.into_iter().enumerate() {
        let members = draws.iter().filter(|d| match key {
            Some(k) => d.forward == Some(k),
            None => d.casts,
        });
        out.buckets.push(bucket(
            &mut out.items,
            &mut out.entries,
            index as u32,
            key,
            members,
        ));
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use concordia_cluster::{ClusterInput, MeshletBuildSettings, MeshletSize};
    use concordia_core::jobs::JobSystem;
    use proptest::prelude::*;

    const INFO: MeshletInfo = MeshletInfo {
        meshlets: 3,
        max_triangles: 128,
    };

    /// A grid of `n`×`n` quads built into meshlets.
    fn blob(n: u32) -> Vec<u8> {
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
        let bytes = blob(16);
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
    fn mesh_table_rows_point_at_the_blob_and_streams() {
        let streams = Streams {
            positions: 11,
            normals: 12,
            tangents: 13,
            uvs: 14,
        };
        assert_eq!(
            mesh_row(10, streams, INFO),
            [[10, 11, 12, 13], [14, 3, 128, 0]]
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
        let p = plan(&draws);
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
                vertices: 384
            }
        );
        assert_eq!(
            p.buckets[1],
            Bucket {
                key: Some(3),
                first: 9,
                entries: 1,
                vertices: 384
            }
        );
        assert_eq!(
            p.buckets[2],
            Bucket {
                key: None,
                first: 10,
                entries: 12,
                vertices: 384
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
    fn vertices_cover_the_largest_meshlets_in_a_bucket() {
        let mut small = draw(1, 2, Some(1), false);
        small.info.max_triangles = 64;
        let p = plan(&[small]);
        assert_eq!(p.buckets[0].vertices, 192);
        let p = plan(&[small, draw(0, 1, Some(1), false)]);
        assert_eq!(p.buckets[0].vertices, 384);
    }

    #[test]
    fn nothing_to_draw_plans_nothing() {
        let p = plan(&[draw(0, 2, None, false)]);
        assert_eq!((p.items.len(), p.buckets.len(), p.entries), (0, 0, 0));
    }

    proptest! {
        #[test]
        fn work_items_cover_every_meshlet_once(
            raw in prop::collection::vec((1u32..40, prop::option::of(0u32..4), any::<bool>()), 0..40)
        ) {
            let draws: Vec<_> = raw.iter().enumerate()
                .map(|(i, (m, k, c))| draw(i as u32, *m, *k, *c))
                .collect();
            let p = plan(&draws);
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
                    .filter(|d| match bucket.key { Some(k) => d.forward == Some(k), None => d.casts })
                    .map(|d| d.info.meshlets)
                    .sum();
                prop_assert_eq!(bucket.entries, want, "bucket {}", b);
            }
            let forward: u32 = draws.iter().filter(|d| d.forward.is_some()).map(|d| d.info.meshlets).sum();
            let shadow: u32 = draws.iter().filter(|d| d.casts).map(|d| d.info.meshlets).sum();
            prop_assert_eq!(p.entries, forward + shadow);
        }
    }
}
