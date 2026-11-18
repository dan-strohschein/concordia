//! World-state hashing: independent of archetype layout, insertion history and thread count.

use std::fmt;

use concordia_core::rng::SimRng;
use concordia_reflect::canonical_hash;

use crate::error::EcsError;
use crate::sim::SimClock;
use crate::world::World;

/// BLAKE3 digest of a world's state.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldHash(pub [u8; 32]);

impl fmt::Display for WorldHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

impl fmt::Debug for WorldHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WorldHash({self})")
    }
}

const DOMAIN: &[u8] = b"concordia-world-hash-v1";

impl World {
    /// Hashes entities (by sorted id), their components (by sorted name), the clock and the RNG.
    pub fn state_hash(&self) -> Result<WorldHash, EcsError> {
        let mut h = blake3::Hasher::new();
        h.update(DOMAIN);
        h.update(&(self.len() as u64).to_le_bytes());
        for id in self.entities.ids() {
            self.hash_entity(id, &mut h)?;
        }
        self.hash_resources(&mut h);
        Ok(WorldHash(*h.finalize().as_bytes()))
    }

    fn hash_entity(
        &self,
        id: concordia_core::id::EntityId,
        h: &mut blake3::Hasher,
    ) -> Result<(), EcsError> {
        let loc = self.entities.location(id)?;
        let arch = &self.archetypes.list[loc.archetype];
        h.update(id.as_uuid().as_bytes());
        h.update(&(arch.components.len() as u64).to_le_bytes());
        for &col in &arch.by_name {
            let name = self.components.info(arch.components[col]).name;
            h.update(&(name.len() as u64).to_le_bytes());
            h.update(name.as_bytes());
            canonical_hash(arch.table.columns[col].get_reflect(loc.row), h)?;
        }
        Ok(())
    }

    fn hash_resources(&self, h: &mut blake3::Hasher) {
        match self.resource::<SimClock>() {
            Some(clock) => {
                h.update(&[1]);
                h.update(&clock.tick.0.to_le_bytes());
                h.update(&clock.rate.hz().to_le_bytes());
            }
            None => {
                h.update(&[0]);
            }
        }
        match self.resource::<SimRng>() {
            Some(rng) => {
                h.update(&[1]);
                h.update(&rng.seed());
                h.update(&rng.stream().to_le_bytes());
                h.update(&rng.word_pos().to_le_bytes());
            }
            None => {
                h.update(&[0]);
            }
        }
    }
}
