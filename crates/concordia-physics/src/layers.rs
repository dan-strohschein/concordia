//! Collision layers (0..=15) and masks.

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// A collision layer, 0..=15 (validated by [`PhysicsLayer::new`] and by the runtime).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Serialize,
    Deserialize,
    Reflect,
)]
#[serde(transparent)]
pub struct PhysicsLayer(pub u8);

impl PhysicsLayer {
    /// Number of layers.
    pub const COUNT: u8 = 16;

    /// The layer, if below [`Self::COUNT`].
    pub fn new(layer: u8) -> Option<Self> {
        (layer < Self::COUNT).then_some(Self(layer))
    }

    /// The index, if valid.
    pub fn index(self) -> Option<u8> {
        (self.0 < Self::COUNT).then_some(self.0)
    }
}

/// A set of layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayerMask(pub u16);

impl LayerMask {
    /// Every layer.
    pub const ALL: Self = Self(u16::MAX);
    /// No layer.
    pub const NONE: Self = Self(0);

    /// The mask with `layer` added.
    pub fn with(self, layer: PhysicsLayer) -> Self {
        Self(self.0 | bit(layer))
    }

    /// The mask with `layer` removed.
    pub fn without(self, layer: PhysicsLayer) -> Self {
        Self(self.0 & !bit(layer))
    }

    /// True if `layer` is in the mask.
    pub fn contains(self, layer: PhysicsLayer) -> bool {
        self.0 & bit(layer) != 0
    }
}

fn bit(layer: PhysicsLayer) -> u16 {
    layer.index().map_or(0, |i| 1 << i)
}
