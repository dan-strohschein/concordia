//! CPU references for the virtual shadow maps' GPU tests.

use concordia_core::math::{IVec2, Vec3, Vec4};

use super::{Clipmap, texel_world};

/// The in-level pages of `level` under the world sphere `sphere` (centre, radius), row by row, as
/// `vsm_sphere_rect` finds them: centre ± radius in texels, one texel wider, clamped to the level.
pub(crate) fn pages_under(clipmap: &Clipmap, level: u32, sphere: Vec4) -> Vec<IVec2> {
    let l = &clipmap.levels[level as usize];
    let clip = l.view_proj * sphere.truncate().extend(1.0);
    let t = Vec3::new(clip.x * 0.5 + 0.5, 0.5 - clip.y * 0.5, 0.0).truncate() * 16_384.0;
    let r = sphere.w / texel_world(level) + 1.0;
    let (lo, hi) = (
        ((t - r) / 128.0).floor().as_ivec2(),
        ((t + r) / 128.0).floor().as_ivec2(),
    );
    if hi.min_element() < 0 || lo.max_element() > 127 {
        return Vec::new();
    }
    let (lo, hi) = (lo.max(IVec2::ZERO), hi.min(IVec2::splat(127)));
    (lo.y..=hi.y)
        .flat_map(|y| (lo.x..=hi.x).map(move |x| IVec2::new(x, y)))
        .collect()
}
