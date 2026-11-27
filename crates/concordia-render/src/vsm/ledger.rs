//! The shadow casters' ledger (M4 4.7b): what each caster looked like last frame and whether it
//! is static or dynamic, so a frame can list the spheres whose static pages went stale and the
//! dynamic casters' spheres.

use std::collections::BTreeMap;

use concordia_assets::Mobility;
use concordia_core::math::{Vec3, Vec4};

/// Frames an `Auto` caster stays dynamic after it last moved.
pub const DYNAMIC_FRAMES: u64 = 60;

/// What a caster looks like to the shadow pages: its world sphere (the one binning and
/// invalidation use) and a hash of its shape (mesh, material, pose).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CasterState {
    /// World centre and radius.
    pub sphere: Vec4,
    /// Hash of what else changes its depth.
    pub shape: u64,
}

/// A caster as the frame sees it: its state, whether it is skinned, and its hint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CasterInput {
    /// Sphere and shape.
    pub state: CasterState,
    /// Skinned casters are always dynamic.
    pub skinned: bool,
    /// `MeshRenderer.mobility`.
    pub hint: Mobility,
}

/// One frame's verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct LedgerFrame<K> {
    /// Spheres whose static pages are stale, in key order.
    pub stale: Vec<Vec4>,
    /// The dynamic casters, in key order.
    pub dynamic: Vec<K>,
    /// Their spheres, in the same order.
    pub dynamic_spheres: Vec<Vec4>,
    /// `Static`-hinted casters that moved this frame.
    pub static_hint_moved: Vec<K>,
}

impl<K> Default for LedgerFrame<K> {
    fn default() -> Self {
        Self {
            stale: Vec::new(),
            dynamic: Vec::new(),
            dynamic_spheres: Vec::new(),
            static_hint_moved: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    state: CasterState,
    dynamic: bool,
    moved_at: Option<u64>,
}

/// Last frame's casters by key.
#[derive(Debug, Clone)]
pub struct CasterLedger<K> {
    last: BTreeMap<K, Entry>,
    frame: u64,
}

impl<K> Default for CasterLedger<K> {
    fn default() -> Self {
        Self {
            last: BTreeMap::new(),
            frame: 0,
        }
    }
}

impl<K: Ord + Clone> CasterLedger<K> {
    /// Records this frame's casters, classifies them and lists what went stale.
    pub fn update(&mut self, now: impl IntoIterator<Item = (K, CasterInput)>) -> LedgerFrame<K> {
        self.frame += 1;
        let frame = self.frame;
        let now: BTreeMap<K, CasterInput> = now.into_iter().collect();
        let keys: std::collections::BTreeSet<&K> = self.last.keys().chain(now.keys()).collect();
        let mut out = LedgerFrame::default();
        let mut next = BTreeMap::new();
        for key in keys {
            let old = self.last.get(key);
            let Some(input) = now.get(key) else {
                // Removed: a static caster leaves its old pages stale.
                out.stale
                    .extend(old.filter(|o| !o.dynamic).map(|o| o.state.sphere));
                continue;
            };
            let changed = old.is_some_and(|o| !same(&o.state, &input.state));
            let moved_at = match changed {
                true => Some(frame),
                false => old.and_then(|o| o.moved_at),
            };
            let dynamic = input.skinned
                || match input.hint {
                    Mobility::Movable => true,
                    Mobility::Static => false,
                    Mobility::Auto => moved_at.is_some_and(|m| frame - m < DYNAMIC_FRAMES),
                };
            let sphere = input.state.sphere;
            match (old.map(|o| (o.dynamic, o.state.sphere)), dynamic) {
                (None, false) => out.stale.push(sphere),
                (Some((false, was)), false) if changed => out.stale.extend([was, sphere]),
                (Some((false, was)), true) => out.stale.push(was),
                (Some((true, _)), false) => out.stale.push(sphere),
                _ => {}
            }
            if dynamic {
                out.dynamic.push(key.clone());
                out.dynamic_spheres.push(sphere);
            }
            if changed && input.hint == Mobility::Static && !input.skinned {
                out.static_hint_moved.push(key.clone());
            }
            next.insert(
                key.clone(),
                Entry {
                    state: input.state,
                    dynamic,
                    moved_at,
                },
            );
        }
        self.last = next;
        out
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
