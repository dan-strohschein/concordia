//! The shadow casters' ledger (M4 4.7b): what each caster looked like last frame, so a frame can
//! list the spheres whose kept pages went stale (moved, added, removed or reshaped casters).

use std::collections::BTreeMap;

use concordia_core::math::{Vec3, Vec4};

/// What a caster looks like to the shadow pages: its world sphere (the one binning and
/// invalidation use) and a hash of its shape (mesh, alpha mode, pose).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CasterState {
    /// World centre and radius.
    pub sphere: Vec4,
    /// Hash of what else changes its depth.
    pub shape: u64,
}

/// Last frame's casters by key.
#[derive(Debug, Clone)]
pub struct CasterLedger<K> {
    last: BTreeMap<K, CasterState>,
}

impl<K> Default for CasterLedger<K> {
    fn default() -> Self {
        Self {
            last: BTreeMap::new(),
        }
    }
}

impl<K: Ord + Clone> CasterLedger<K> {
    /// Records this frame's casters and returns the spheres whose pages are stale, in key order.
    pub fn diff(&mut self, now: impl IntoIterator<Item = (K, CasterState)>) -> Vec<Vec4> {
        let now: BTreeMap<K, CasterState> = now.into_iter().collect();
        let keys: std::collections::BTreeSet<&K> = self.last.keys().chain(now.keys()).collect();
        let mut stale = Vec::new();
        for key in keys {
            match (self.last.get(key), now.get(key)) {
                (Some(old), Some(new)) if same(old, new) => {}
                (old, new) => stale.extend(old.into_iter().chain(new).map(|c| c.sphere)),
            }
        }
        self.last = now;
        stale
    }

    /// Forgets every caster (the cache was cleared).
    pub fn clear(&mut self) {
        self.last.clear();
    }
}

/// A caster's box for invalidation: its mesh's bounds `aabb` grown to hold each meshlet sphere
/// (centre, radius) in `spheres`, all in object space.
pub fn caster_box(
    aabb: (Vec3, Vec3),
    spheres: impl IntoIterator<Item = (Vec3, f32)>,
) -> (Vec3, Vec3) {
    spheres.into_iter().fold(aabb, |(lo, hi), (c, r)| {
        (lo.min(c - Vec3::splat(r)), hi.max(c + Vec3::splat(r)))
    })
}

/// Whether two states are the same bit for bit.
fn same(a: &CasterState, b: &CasterState) -> bool {
    let bits = |v: Vec4| v.to_array().map(f32::to_bits);
    a.shape == b.shape && bits(a.sphere) == bits(b.sphere)
}
