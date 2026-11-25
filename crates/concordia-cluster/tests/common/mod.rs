//! Shared mesh generators for the cluster tests.
#![allow(dead_code)]

use std::num::NonZeroUsize;

use concordia_core::jobs::JobSystem;
use proptest::prelude::*;

/// An owned test mesh.
#[derive(Debug, Clone)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub materials: Option<Vec<u16>>,
}

impl Mesh {
    pub fn input(&self) -> concordia_cluster::ClusterInput<'_> {
        concordia_cluster::ClusterInput::new(
            &self.positions,
            &self.indices,
            self.materials.as_deref(),
        )
        .unwrap()
    }
}

pub fn jobs(threads: usize) -> JobSystem {
    JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap()
}

/// A random triangle soup with optional materials (0..4), degenerate triangles included.
pub fn soup() -> impl Strategy<Value = Mesh> {
    (
        prop::collection::vec(prop::array::uniform3(-5.0f32..5.0), 3..120),
        prop::collection::vec(any::<prop::sample::Index>(), 3..900),
        prop::bool::ANY,
        prop::collection::vec(0u16..4, 300),
    )
        .prop_map(|(positions, picks, with_materials, mats)| {
            let count = picks.len() / 3 * 3;
            let indices = picks[..count]
                .iter()
                .map(|i| i.index(positions.len()) as u32)
                .collect::<Vec<_>>();
            let materials =
                with_materials.then(|| (0..count / 3).map(|t| mats[t % mats.len()]).collect());
            Mesh {
                positions,
                indices,
                materials,
            }
        })
}

/// A bumpy `n`×`n` grid patch facing +Z (CCW from above), rotated by `angles`.
pub fn patch(n: u32, heights: &[f32], angles: [f32; 2], offset: [f32; 3]) -> Mesh {
    let (sa, ca) = angles[0].sin_cos();
    let (sb, cb) = angles[1].sin_cos();
    let mut positions = Vec::new();
    for y in 0..=n {
        for x in 0..=n {
            let h = heights[((y * (n + 1) + x) as usize) % heights.len()];
            let p = [x as f32 * 0.1, y as f32 * 0.1, h * 0.02];
            // Rotate about X, then Z.
            let q = [p[0], p[1] * ca - p[2] * sa, p[1] * sa + p[2] * ca];
            let r = [q[0] * cb - q[1] * sb, q[0] * sb + q[1] * cb, q[2]];
            positions.push([r[0] + offset[0], r[1] + offset[1], r[2] + offset[2]]);
        }
    }
    let mut indices = Vec::new();
    let w = n + 1;
    for y in 0..n {
        for x in 0..n {
            let i = y * w + x;
            indices.extend([i, i + 1, i + w + 1, i, i + w + 1, i + w]);
        }
    }
    Mesh {
        positions,
        indices,
        materials: None,
    }
}

pub fn patch_strategy() -> impl Strategy<Value = Mesh> {
    (
        4u32..40,
        prop::collection::vec(-1.0f32..1.0, 1..64),
        prop::array::uniform2(-3.2f32..3.2),
        prop::array::uniform3(-3.0f32..3.0),
    )
        .prop_map(|(n, h, a, o)| patch(n, &h, a, o))
}

/// Soups or patches.
pub fn any_mesh() -> impl Strategy<Value = Mesh> {
    prop_oneof![soup(), patch_strategy()]
}

/// A triangle with its corners rotated so the smallest index is first (winding kept).
pub fn canonical(t: [u32; 3]) -> [u32; 3] {
    let m = (0..3).min_by_key(|&i| t[i]).unwrap();
    [t[m], t[(m + 1) % 3], t[(m + 2) % 3]]
}
