//! World-state hashing: independent of archetype layout, insertion history and thread count.

use std::collections::BTreeMap;
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

/// A world hash plus one hash per part: each hashed component by name, and `@entities`,
/// `@clock`, `@rng` and `@signals`; a divergence report names the parts that differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDigest {
    /// The world hash ([`World::state_hash`]).
    pub world: WorldHash,
    /// Hashes by part name.
    pub parts: BTreeMap<String, WorldHash>,
}

const DOMAIN: &[u8] = b"concordia-world-hash-v1";

/// Per-part hashers, filled alongside the world hash.
#[derive(Default)]
struct Parts(BTreeMap<String, blake3::Hasher>);

impl Parts {
    fn part(&mut self, name: &str) -> &mut blake3::Hasher {
        if !self.0.contains_key(name) {
            let mut h = blake3::Hasher::new();
            h.update(DOMAIN);
            h.update(name.as_bytes());
            self.0.insert(name.to_owned(), h);
        }
        self.0.get_mut(name).expect("inserted above")
    }

    fn finish(self) -> BTreeMap<String, WorldHash> {
        self.0
            .into_iter()
            .map(|(name, h)| (name, WorldHash(*h.finalize().as_bytes())))
            .collect()
    }
}

impl World {
    /// Hashes entities (by sorted id), their components (by sorted name), the clock and the RNG.
    pub fn state_hash(&self) -> Result<WorldHash, EcsError> {
        self.hash_into(None)
    }

    /// The world hash and the hash of each part, in one pass.
    pub fn state_digest(&self) -> Result<StateDigest, EcsError> {
        let mut parts = Parts::default();
        let world = self.hash_into(Some(&mut parts))?;
        Ok(StateDigest {
            world,
            parts: parts.finish(),
        })
    }

    fn hash_into(&self, mut parts: Option<&mut Parts>) -> Result<WorldHash, EcsError> {
        let mut h = blake3::Hasher::new();
        h.update(DOMAIN);
        h.update(&(self.len() as u64).to_le_bytes());
        let presentation: Vec<crate::component::ComponentId> = self
            .presentation
            .iter()
            .filter_map(|name| self.components.by_name(name))
            .collect();
        for id in self.entities.ids() {
            self.hash_entity(id, &presentation, &mut h, parts.as_deref_mut())?;
        }
        self.hash_clock(&mut h);
        self.hash_rng(&mut h);
        self.signals.hash(&mut h)?;
        if let Some(parts) = parts {
            self.hash_clock(parts.part("@clock"));
            self.hash_rng(parts.part("@rng"));
            self.signals.hash(parts.part("@signals"))?;
        }
        Ok(WorldHash(*h.finalize().as_bytes()))
    }

    fn hash_entity(
        &self,
        id: concordia_core::id::EntityId,
        presentation: &[crate::component::ComponentId],
        h: &mut blake3::Hasher,
        mut parts: Option<&mut Parts>,
    ) -> Result<(), EcsError> {
        let loc = self.entities.location(id)?;
        let arch = &self.archetypes.list[loc.archetype];
        h.update(id.as_uuid().as_bytes());
        if let Some(parts) = parts.as_deref_mut() {
            parts.part("@entities").update(id.as_uuid().as_bytes());
        }
        // Every storage kind hashes alike: by name, then value (ADR-0069).
        let hashed: Vec<_> = arch
            .by_name
            .iter()
            .map(|&slot| arch.component_at(slot))
            .filter(|c| !presentation.contains(c))
            .collect();
        h.update(&(hashed.len() as u64).to_le_bytes());
        for cid in hashed {
            let name = self.components.info(cid).name;
            h.update(&(name.len() as u64).to_le_bytes());
            h.update(name.as_bytes());
            let value = self
                .reflect_of(loc, cid)
                .expect("the archetype has the component");
            canonical_hash(value, h)?;
            if let Some(parts) = parts.as_deref_mut() {
                let part = parts.part(name);
                part.update(id.as_uuid().as_bytes());
                canonical_hash(value, part)?;
            }
        }
        Ok(())
    }

    fn hash_clock(&self, h: &mut blake3::Hasher) {
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
    }

    fn hash_rng(&self, h: &mut blake3::Hasher) {
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
