//! Which shadowed local lights get virtual shadow maps this frame (M4 4.7c; M10's atlas reuses
//! it): the top `max` by importance (screen coverage × candela), ties by key, with hysteresis so
//! slots don't flicker, and stable slots.

use concordia_core::math::Vec3;

/// A selected light keeps its slot unless an outsider beats it by more than this factor.
pub const HYSTERESIS: f32 = 1.1;

/// What one frame's selection changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picked<K> {
    /// Keys that got a slot.
    pub entered: Vec<K>,
    /// Keys that lost theirs.
    pub left: Vec<K>,
    /// Candidates beyond the cap (shaded unshadowed).
    pub dropped: usize,
}

/// The slots and the keys holding them.
#[derive(Debug, Clone)]
pub struct LocalSelection<K> {
    slots: Vec<Option<K>>,
}

impl<K: Ord + Clone> LocalSelection<K> {
    /// `max` empty slots.
    pub fn new(max: usize) -> Self {
        Self {
            slots: vec![None; max],
        }
    }

    /// Selects from `candidates` (key, importance) and returns what changed.
    pub fn select(&mut self, candidates: &[(K, f32)]) -> Picked<K> {
        let held = |k: &K| self.slot_of(k).is_some();
        let mut ranked: Vec<(&K, f32)> = candidates
            .iter()
            .filter(|(_, i)| *i > 0.0)
            .map(|(k, i)| (k, if held(k) { i * HYSTERESIS } else { *i }))
            .collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        let dropped = ranked.len().saturating_sub(self.slots.len());
        ranked.truncate(self.slots.len());
        let chosen: Vec<&K> = ranked.iter().map(|(k, _)| *k).collect();
        let mut left = Vec::new();
        for slot in &mut self.slots {
            if slot.as_ref().is_some_and(|k| !chosen.contains(&k)) {
                left.extend(slot.take());
            }
        }
        let mut entered = Vec::new();
        for k in chosen {
            if self.slot_of(k).is_none()
                && let Some(free) = self.slots.iter_mut().find(|s| s.is_none())
            {
                *free = Some(k.clone());
                entered.push(k.clone());
            }
        }
        left.sort();
        Picked {
            entered,
            left,
            dropped,
        }
    }

    /// The key in `slot`.
    pub fn key(&self, slot: usize) -> Option<&K> {
        self.slots.get(slot).and_then(Option::as_ref)
    }

    /// The slot holding `key`.
    pub fn slot_of(&self, key: &K) -> Option<usize> {
        self.slots.iter().position(|s| s.as_ref() == Some(key))
    }
}

/// A light's importance: the share of a view (vertical field of view `fov_y`, at `eye`) its range
/// sphere covers, at most 1, times its candela.
pub fn importance(position: Vec3, range: f32, lumens: f32, eye: Vec3, fov_y: f32) -> f32 {
    let d = position.distance(eye);
    let coverage = match d > range {
        true => {
            let radius = (range / d).asin().tan() / (fov_y / 2.0).tan().max(1e-6);
            (radius * radius).min(1.0)
        }
        false => 1.0,
    };
    coverage * concordia_render_types::candela(lumens).max(0.0)
}
