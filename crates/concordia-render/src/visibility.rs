//! Persistent per-meshlet visibility (M4 4.4b, ADR-0100): which meshlets the camera saw last frame,
//! so two-pass occlusion draws those first. Each meshlet-path instance (by key: entity, mesh and
//! occurrence) owns a run of words in one GPU buffer, one word per meshlet (a word, not a bit: no
//! atomics, so two threads never share one). Bases are stable while a key keeps being drawn; keys
//! not drawn this frame are pruned, as the motion history forgets entities, and their runs reused.
//! A stale or fresh run only changes what pass 1 draws, never what is drawn in the end.

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use concordia_assets::AssetRef;
use concordia_core::id::EntityId;
use concordia_rhi::{Buffer, BufferDesc, BufferUsage, Device, MemoryLocation};

use crate::RenderError;
use crate::gpu::Uploads;

/// A meshlet-path instance's key: its entity, its mesh, and which of the entity's draws of that
/// mesh it is.
pub(crate) type VisKey = (EntityId, AssetRef, u32);

/// Fewest words the GPU buffer holds.
const MIN_WORDS: u32 = 1024;

/// A run of words: its first word and its length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    base: u32,
    words: u32,
}

/// The bases of every key's run.
#[derive(Debug, Clone)]
pub(crate) struct Visibility<K> {
    runs: BTreeMap<K, Run>,
    /// Words in use or freed (the buffer needs at least this many).
    end: u32,
}

impl<K> Default for Visibility<K> {
    fn default() -> Self {
        Self {
            runs: BTreeMap::new(),
            end: 0,
        }
    }
}

impl<K: Ord + Clone> Visibility<K> {
    /// Gives each `(key, meshlets)` its run (kept for keys it had last time with the same
    /// length), drops every other key, and returns the bases in order.
    pub(crate) fn assign(&mut self, wanted: &[(K, u32)]) -> Vec<u32> {
        let mut runs = BTreeMap::new();
        for (key, words) in wanted {
            if let Some(run) = self.runs.get(key).filter(|r| r.words == *words) {
                runs.insert(key.clone(), *run);
            }
        }
        let mut taken: Vec<Run> = runs.values().copied().collect();
        taken.sort_by_key(|r| r.base);
        for (key, words) in wanted {
            if !runs.contains_key(key) {
                let run = Run {
                    base: first_fit(&taken, *words),
                    words: *words,
                };
                let at = taken.partition_point(|r| r.base < run.base);
                taken.insert(at, run);
                runs.insert(key.clone(), run);
            }
        }
        debug_assert_eq!(runs.len(), wanted.len(), "keys are unique");
        self.end = taken.last().map_or(0, |r| r.base + r.words);
        self.runs = runs;
        wanted.iter().map(|(key, _)| self.runs[key].base).collect()
    }

    /// Words the buffer must hold.
    pub(crate) fn words(&self) -> u32 {
        self.end
    }
}

/// The GPU buffer of the visibility words, grown as the runs need; a new one starts zeroed (so
/// pass 1 draws nothing of it).
#[derive(Debug, Default)]
pub(crate) struct VisibilityBuffer {
    current: Option<(Buffer, u32)>,
}

impl VisibilityBuffer {
    /// A buffer of at least `words` words: the current one, or a zeroed larger one (the current
    /// one handed to `retire`, to be freed once its frames are done).
    pub(crate) fn ensure(
        &mut self,
        device: &dyn Device,
        up: &mut Uploads,
        words: u32,
        retire: &mut dyn FnMut(Buffer),
    ) -> Result<Buffer, RenderError> {
        if let Some((buffer, capacity)) = self.current {
            if capacity >= words {
                return Ok(buffer);
            }
            retire(buffer);
            self.current = None;
        }
        let capacity = words.max(MIN_WORDS).next_power_of_two();
        let bytes = u64::from(capacity) * 4;
        let buffer = device.create_buffer(&BufferDesc {
            size: NonZeroU64::new(bytes).unwrap_or(NonZeroU64::MIN),
            usage: BufferUsage::STORAGE | BufferUsage::BINDLESS | BufferUsage::COPY_DST,
            memory: MemoryLocation::GpuOnly,
            label: "meshlet visibility".into(),
        })?;
        self.current = Some((buffer, capacity));
        up.write_into(device, buffer, &vec![0; bytes as usize])?;
        Ok(buffer)
    }

    /// Frees the buffer (the device must be idle).
    pub(crate) fn destroy(&mut self, device: &dyn Device) {
        if let Some((buffer, _)) = self.current.take() {
            device.destroy_buffer(buffer);
        }
    }
}

/// The first gap of at least `words` between the runs `taken` (sorted by base), or their end.
fn first_fit(taken: &[Run], words: u32) -> u32 {
    let mut at = 0;
    for run in taken {
        if run.base - at >= words {
            return at;
        }
        at = run.base + run.words;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn visibility_bases_are_stable_and_pruned() {
        let mut v = Visibility::default();
        assert_eq!(v.assign(&[("a", 10), ("b", 4), ("c", 7)]), [0, 10, 14]);
        assert_eq!(v.words(), 21);
        // Kept keys keep their bases whatever the order; b is pruned.
        assert_eq!(v.assign(&[("c", 7), ("a", 10)]), [14, 0]);
        // A new key takes the first freed run it fits; one that fits none goes at the end.
        assert_eq!(
            v.assign(&[("a", 10), ("c", 7), ("d", 3), ("e", 5)]),
            [0, 14, 10, 21]
        );
        assert_eq!(v.words(), 26);
        // A key whose meshlet count changed gets a new run.
        assert_eq!(v.assign(&[("a", 12)]), [0]);
        assert_eq!(v.assign(&[("a", 30)]), [0]);
        assert_eq!(v.words(), 30);
        assert_eq!(v.assign(&[]), Vec::<u32>::new());
        assert_eq!(v.assign(&[("z", 1)]), [0]);
    }

    proptest! {
        /// Runs never overlap, fit in `words`, and keys kept with their length keep their base.
        #[test]
        fn runs_are_disjoint_and_stable(
            frames in prop::collection::vec(
                prop::collection::btree_map(0u8..24, 1u32..40, 0..12), 1..12),
        ) {
            let mut v = Visibility::default();
            let mut last: BTreeMap<u8, (u32, u32)> = BTreeMap::new();
            for frame in frames {
                let wanted: Vec<(u8, u32)> = frame.into_iter().collect();
                let bases = v.assign(&wanted);
                prop_assert_eq!(bases.len(), wanted.len());
                let mut spans: Vec<(u32, u32)> = wanted
                    .iter()
                    .zip(&bases)
                    .map(|((_, n), b)| (*b, b + n))
                    .collect();
                spans.sort();
                prop_assert!(spans.windows(2).all(|w| w[0].1 <= w[1].0), "{spans:?}");
                prop_assert!(spans.iter().all(|s| s.1 <= v.words()));
                for ((key, n), base) in wanted.iter().zip(&bases) {
                    if let Some((b, _)) = last.get(key).filter(|(_, m)| m == n) {
                        prop_assert_eq!(b, base);
                    }
                }
                last = wanted.iter().zip(&bases).map(|((k, n), b)| (*k, (*b, *n))).collect();
            }
        }
    }
}
