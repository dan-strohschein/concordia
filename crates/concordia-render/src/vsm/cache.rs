//! Virtual shadow maps' cache across frames (M4 4.7b): when kept pages stop being valid as a whole
//! (a new view, sun direction, levels, pool or depth range; the cache off; an explicit reset).

use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// What kept pages depend on as a whole; a change clears the cache.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CacheKey {
    /// The view the pages were requested for (one cached view at a time).
    pub view: u64,
    /// The sun's travel direction (compared bit for bit).
    pub light_dir: Vec3,
    /// Clipmap levels.
    pub levels: u8,
    /// Physical pages.
    pub pool_pages: u32,
    /// The light-space depth range (far, near) stored depth is relative to.
    pub depth: (f32, f32),
}

/// Why the cache was cleared.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum CacheReset {
    /// Nothing was cached yet.
    First,
    /// Another view was drawn.
    View,
    /// The sun turned.
    Light,
    /// `levels` or `pool_pages` changed.
    Settings,
    /// The casters outgrew the depth range.
    DepthRange,
    /// `cache = false`: every frame starts empty.
    Off,
    /// A scene was opened or re-sent.
    Scene,
    /// The shadow technique changed.
    Technique,
}

/// The key the cache was built for and the reset waiting for the next frame.
#[derive(Debug, Default, Clone)]
pub struct CacheState {
    key: Option<CacheKey>,
    pending: Option<CacheReset>,
    last: Option<CacheReset>,
    was_off: bool,
}

impl CacheState {
    /// Starts a frame with `key`; returns why the cache must be cleared first, if it must.
    pub fn begin(&mut self, key: CacheKey, cache: bool) -> Option<CacheReset> {
        let changed = match self.key {
            None => Some(CacheReset::First),
            Some(old) => change(&old, &key),
        };
        let why = match (cache, self.pending.take()) {
            (false, _) => Some(CacheReset::Off),
            (true, Some(p)) => Some(p),
            (true, None) if self.was_off => Some(CacheReset::Off),
            (true, None) => changed,
        };
        self.key = Some(key);
        self.was_off = !cache;
        if why.is_some() {
            self.last = why;
        }
        why
    }

    /// Clears the cache at the next frame for `why`.
    pub fn reset(&mut self, why: CacheReset) {
        self.pending = Some(why);
    }

    /// The last reason the cache was cleared.
    pub fn last_reset(&self) -> Option<CacheReset> {
        self.last
    }
}

/// Why kept pages stop matching when the key goes from `old` to `new`, if they do.
fn change(old: &CacheKey, new: &CacheKey) -> Option<CacheReset> {
    let bits = |v: Vec3| v.to_array().map(f32::to_bits);
    if old.view != new.view {
        Some(CacheReset::View)
    } else if bits(old.light_dir) != bits(new.light_dir) {
        Some(CacheReset::Light)
    } else if (old.levels, old.pool_pages) != (new.levels, new.pool_pages) {
        Some(CacheReset::Settings)
    } else if old.depth.0.to_bits() != new.depth.0.to_bits()
        || old.depth.1.to_bits() != new.depth.1.to_bits()
    {
        Some(CacheReset::DepthRange)
    } else {
        None
    }
}
