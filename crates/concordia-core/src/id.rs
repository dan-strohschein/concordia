//! Typed UUID ids and the generators that create them.
//!
//! Engine-created ids are UUIDv7 (time-ordered). Imported content uses
//! name-based UUIDv5 (see `plans/README.md`), so parsing accepts both.

use std::fmt;
use std::str::FromStr;

pub use uuid::Uuid;

/// Errors from id parsing and generation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    /// The text is not a UUID.
    #[error("invalid id {input:?}: {reason}")]
    Parse {
        /// The rejected text.
        input: String,
        /// Why it was rejected.
        reason: String,
    },
    /// The UUID version is neither 7 (engine) nor 5 (imported).
    #[error("id has UUID version {found}; expected version 7 (or 5 for imported content)")]
    WrongVersion {
        /// The version found.
        found: usize,
    },
    /// The UUID variant is not RFC 9562.
    #[error("id is not an RFC 9562 UUID")]
    WrongVariant,
    /// A deterministic generator ran past the 48-bit timestamp range.
    #[error("deterministic id generator exhausted its timestamp range")]
    Exhausted,
    /// A generator was configured with an out-of-range value.
    #[error("invalid id generator config: {0}")]
    Config(String),
}

/// Checks that `uuid` is an RFC 9562 v7 or v5 UUID.
pub fn validate(uuid: Uuid) -> Result<Uuid, IdError> {
    if uuid.get_variant() != uuid::Variant::RFC4122 {
        return Err(IdError::WrongVariant);
    }
    match uuid.get_version_num() {
        5 | 7 => Ok(uuid),
        found => Err(IdError::WrongVersion { found }),
    }
}

/// A typed id wrapping a validated UUID.
pub trait TypedId: Copy + Ord + fmt::Display + FromStr<Err = IdError> {
    /// Wraps `uuid` after validating it.
    fn try_from_uuid(uuid: Uuid) -> Result<Self, IdError>;
    /// The underlying UUID.
    fn as_uuid(&self) -> Uuid;
}

macro_rules! define_id {
    ($($(#[$doc:meta])* $name:ident),+ $(,)?) => {$(
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        pub struct $name(Uuid);

        impl $name {
            /// The underlying UUID.
            pub fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl TypedId for $name {
            fn try_from_uuid(uuid: Uuid) -> Result<Self, IdError> {
                validate(uuid).map(Self)
            }
            fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = IdError;
            fn try_from(uuid: Uuid) -> Result<Self, IdError> {
                <Self as TypedId>::try_from_uuid(uuid)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.hyphenated().fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = IdError;
            fn from_str(s: &str) -> Result<Self, IdError> {
                parse_uuid(s).and_then(<Self as TypedId>::try_from_uuid)
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = <std::borrow::Cow<'de, str>>::deserialize(d)?;
                text.parse().map_err(serde::de::Error::custom)
            }
        }
    )+};
}

define_id!(
    /// Stable id of an entity; survives save, load and undo.
    EntityId,
    /// Stable id of a scene file.
    SceneId,
    /// Id of a command transaction.
    TxnId,
    /// Id of an Engine API session.
    SessionId,
    /// Stable id of a prefab file.
    PrefabId,
    /// Stable id of an asset.
    AssetId,
);

fn parse_uuid(s: &str) -> Result<Uuid, IdError> {
    Uuid::parse_str(s).map_err(|e| IdError::Parse {
        input: s.to_owned(),
        reason: e.to_string(),
    })
}

/// A source of fresh UUIDv7 values.
///
/// ```
/// use concordia_core::id::{DeterministicIdGen, EntityId, IdSource};
/// let mut ids = DeterministicIdGen::new(42);
/// let a: EntityId = ids.next_id()?;
/// let b: EntityId = ids.next_id()?;
/// assert!(a < b);
/// assert_eq!(a.to_string().parse::<EntityId>()?, a);
/// # Ok::<(), concordia_core::id::IdError>(())
/// ```
pub trait IdSource {
    /// Returns the next UUID.
    fn next_uuid(&mut self) -> Result<Uuid, IdError>;

    /// Returns the next UUID as a typed id.
    fn next_id<I: TypedId>(&mut self) -> Result<I, IdError> {
        self.next_uuid().and_then(I::try_from_uuid)
    }
}

/// Production ids from the system clock, ordered within a process.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemIdGen;

impl IdSource for SystemIdGen {
    fn next_uuid(&mut self) -> Result<Uuid, IdError> {
        Ok(Uuid::now_v7())
    }
}

/// Reproducible, strictly increasing UUIDv7 ids from a seed (tests, sim, `--deterministic-ids`).
#[derive(Debug, Clone)]
pub struct DeterministicIdGen {
    seed: u64,
    base_ms: u64,
    counter: u64,
}

const COUNTER_BITS: u32 = 12;
const COUNTER_MASK: u64 = (1 << COUNTER_BITS) - 1;

impl DeterministicIdGen {
    /// Default timestamp base: 2023-11-14T22:13:20Z.
    pub const DEFAULT_BASE_MS: u64 = 1_700_000_000_000;
    /// Largest timestamp a UUIDv7 can hold (48 bits).
    pub const MAX_BASE_MS: u64 = (1 << 48) - 1;

    /// Creates a generator with the default timestamp base.
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            base_ms: Self::DEFAULT_BASE_MS,
            counter: 0,
        }
    }

    /// Creates a generator whose ids start at `base_ms` (Unix milliseconds).
    pub fn with_base_ms(seed: u64, base_ms: u64) -> Result<Self, IdError> {
        if base_ms > Self::MAX_BASE_MS {
            return Err(IdError::Config(format!(
                "base_ms {base_ms} exceeds the 48-bit maximum {}",
                Self::MAX_BASE_MS
            )));
        }
        Ok(Self {
            seed,
            base_ms,
            counter: 0,
        })
    }

    fn timestamp_ms(&self) -> Result<u64, IdError> {
        self.base_ms
            .checked_add(self.counter >> COUNTER_BITS)
            .filter(|&ms| ms <= Self::MAX_BASE_MS)
            .ok_or(IdError::Exhausted)
    }
}

impl IdSource for DeterministicIdGen {
    fn next_uuid(&mut self) -> Result<Uuid, IdError> {
        let ms = self.timestamp_ms()?;
        let rand_b = splitmix64(self.seed ^ splitmix64(self.counter)) >> 2;
        let bits = (u128::from(ms) << 80)
            | (0x7 << 76)
            | (u128::from(self.counter & COUNTER_MASK) << 64)
            | (0b10 << 62)
            | u128::from(rand_b);
        self.counter = self.counter.checked_add(1).ok_or(IdError::Exhausted)?;
        Ok(Uuid::from_u128(bits))
    }
}

/// SplitMix64 finalizer (Steele, Lea & Flood, "Fast splittable PRNGs", 2014).
fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_layout_puts_counter_in_rand_a() {
        let mut ids = DeterministicIdGen::new(0);
        let first = ids.next_uuid().unwrap().as_u128();
        let second = ids.next_uuid().unwrap().as_u128();
        assert_eq!((first >> 64) & 0xFFF, 0);
        assert_eq!((second >> 64) & 0xFFF, 1);
        assert_eq!(first >> 80, u128::from(DeterministicIdGen::DEFAULT_BASE_MS));
    }

    #[test]
    fn counter_rollover_advances_timestamp() {
        let mut ids = DeterministicIdGen::new(0);
        let last = (0..=4096).map(|_| ids.next_uuid().unwrap()).last().unwrap();
        assert_eq!(
            last.as_u128() >> 80,
            u128::from(DeterministicIdGen::DEFAULT_BASE_MS + 1)
        );
    }
}
