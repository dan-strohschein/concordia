//! World-wide physics settings.

use concordia_core::math::Vec3;

use crate::layers::{LayerMask, PhysicsLayer};

/// Gravity, capacity and which layers collide.
#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsSettings {
    /// Gravity in m/s².
    pub gravity: Vec3,
    /// Most bodies a world can hold (1..=65536).
    pub max_bodies: u32,
    collide: [LayerMask; PhysicsLayer::COUNT as usize],
}

impl Default for PhysicsSettings {
    fn default() -> Self {
        Self {
            gravity: Vec3::new(0.0, -9.81, 0.0),
            max_bodies: 16_384,
            collide: [LayerMask::ALL; PhysicsLayer::COUNT as usize],
        }
    }
}

impl PhysicsSettings {
    /// Makes `a` and `b` collide or not (symmetric); invalid layers are ignored.
    pub fn set_collides(&mut self, a: PhysicsLayer, b: PhysicsLayer, collide: bool) {
        let (Some(ia), Some(ib)) = (a.index(), b.index()) else {
            return;
        };
        let toggle = |mask: LayerMask, other| match collide {
            true => mask.with(other),
            false => mask.without(other),
        };
        self.collide[ia as usize] = toggle(self.collide[ia as usize], b);
        self.collide[ib as usize] = toggle(self.collide[ib as usize], a);
    }

    /// True if `a` and `b` collide.
    pub fn collides(&self, a: PhysicsLayer, b: PhysicsLayer) -> bool {
        a.index()
            .is_some_and(|i| self.collide[i as usize].contains(b))
    }

    /// The per-layer masks (Jolt's collide matrix).
    pub(crate) fn matrix(&self) -> [u16; PhysicsLayer::COUNT as usize] {
        self.collide.map(|m| m.0)
    }
}
