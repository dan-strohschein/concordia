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

/// A hash string was not 64 hex digits.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid world hash {0:?}: expected 64 hex digits")]
pub struct ParseHashError(pub String);

impl std::str::FromStr for WorldHash {
    type Err = ParseHashError;

    fn from_str(s: &str) -> Result<Self, ParseHashError> {
        let err = || ParseHashError(s.to_owned());
        if s.len() != 64 || !s.is_ascii() {
            return Err(err());
        }
        let mut bytes = [0u8; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|_| err())?;
        }
        Ok(Self(bytes))
    }
}

impl serde::Serialize for WorldHash {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for WorldHash {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

const DOMAIN: &[u8] = b"concordia-world-hash-v1";

impl World {
    /// Hashes entities (by sorted id), their components (by sorted name), the clock and the RNG.
    pub fn state_hash(&self) -> Result<WorldHash, EcsError> {
        let mut h = blake3::Hasher::new();
        h.update(DOMAIN);
        h.update(&(self.len() as u64).to_le_bytes());
        let presentation: Vec<crate::component::ComponentId> = self
            .presentation
            .iter()
            .filter_map(|t| self.components.id_of(*t))
            .collect();
        for id in self.entities.ids() {
            self.hash_entity(id, &presentation, &mut h)?;
        }
        self.hash_resources(&mut h);
        Ok(WorldHash(*h.finalize().as_bytes()))
    }

    fn hash_entity(
        &self,
        id: concordia_core::id::EntityId,
        presentation: &[crate::component::ComponentId],
        h: &mut blake3::Hasher,
    ) -> Result<(), EcsError> {
        let loc = self.entities.location(id)?;
        let arch = &self.archetypes.list[loc.archetype];
        h.update(id.as_uuid().as_bytes());
        let hashed: Vec<usize> = arch
            .by_name
            .iter()
            .copied()
            .filter(|&col| !presentation.contains(&arch.components[col]))
            .collect();
        h.update(&(hashed.len() as u64).to_le_bytes());
        for col in hashed {
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
                h.update(&clock.time_scale.get().to_le_bytes());
                h.update(&clock.time.to_le_bytes());
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
