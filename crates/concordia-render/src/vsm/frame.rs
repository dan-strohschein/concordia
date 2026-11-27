//! The per-frame VSM blocks: the parameter rows (`VsmParams` in `vsm.slang`) and the depth views
//! (the camera, then one per clipmap level).

use concordia_core::math::{Mat4, Vec3, Vec4};

use super::clipmap::{Clipmap, LEVEL0_EXTENT, texel_world};
use super::settings::VsmSettings;
use crate::frame::{columns, push_rows, push_rows_u32};

/// Row of the first level's origin (`VSM_ROW_LEVELS`).
pub(crate) const ROW_LEVELS: usize = 12;
/// Most pool buffers (16 384 pages of 1024).
pub(crate) const MAX_CHUNKS: usize = 16;

/// How big one screen pixel is in the world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Footprint {
    /// Perspective: metres per pixel per metre of distance (`2 tan(fov/2) / height`).
    PerMetre(f32),
    /// Orthographic: metres per pixel.
    Fixed(f32),
}

/// What the parameter block holds.
pub(crate) struct ParamsIn<'a> {
    pub(crate) settings: &'a VsmSettings,
    pub(crate) clipmap: &'a Clipmap,
    pub(crate) eye: Vec3,
    pub(crate) footprint: Footprint,
    pub(crate) pcf_radius: u32,
    /// Bindless indices of the pool buffers.
    pub(crate) chunks: &'a [u32],
    /// The camera's (unjittered) inverse view-projection.
    pub(crate) inv_view_proj: Mat4,
}

/// The coarse floor: the finest level at least eight times `max_distance` wide, so a frame's
/// floor pages stay few (at most a 16-page radius around the camera), clamped to `levels`.
pub fn floor_level(max_distance: f32, levels: u8) -> u32 {
    let need = (8.0 * max_distance / LEVEL0_EXTENT).max(1.0).log2().ceil();
    (need as u32).min(u32::from(levels.max(1)) - 1)
}

/// The parameter rows (mirrors `VsmParams` and `vsm_origin` in `vsm.slang`).
pub(crate) fn params_rows(p: &ParamsIn<'_>) -> Vec<u8> {
    debug_assert!(
        p.chunks.len() <= MAX_CHUNKS,
        "{} pool chunks",
        p.chunks.len()
    );
    let levels = p.clipmap.levels.len() as u32;
    let floor = floor_level(p.settings.max_distance, p.settings.levels);
    let mut out = Vec::new();
    push_rows_u32(
        &mut out,
        &[[levels, floor, p.settings.pool_pages, p.pcf_radius]],
    );
    let (per_metre, fixed) = match p.footprint {
        Footprint::PerMetre(m) => (m, 0.0),
        Footprint::Fixed(f) => (0.0, f),
    };
    push_rows(
        &mut out,
        &[
            Vec4::new(texel_world(0), p.settings.resolution_bias, per_metre, fixed),
            Vec4::new(
                p.settings.depth_bias,
                p.settings.normal_bias,
                p.settings.max_distance,
                0.0,
            ),
            p.eye.extend(1.0),
        ],
    );
    let mut chunks = [0u32; MAX_CHUNKS];
    chunks[..p.chunks.len()].copy_from_slice(p.chunks);
    push_rows_u32(&mut out, chunks.as_chunks::<4>().0);
    push_rows(&mut out, &columns(&p.inv_view_proj));
    let origins: Vec<[u32; 4]> = p
        .clipmap
        .levels
        .iter()
        .map(|l| {
            let o = l.origin_page();
            [o.x as u32, o.y as u32, p.clipmap.depth_span().to_bits(), 0]
        })
        .collect();
    push_rows_u32(&mut out, &origins);
    debug_assert_eq!(out.len(), (ROW_LEVELS + p.clipmap.levels.len()) * 16);
    out
}

/// The depth views: the camera's view-projection, then each level's (`cascade_xform` rows).
pub(crate) fn depth_views(camera: &Mat4, clipmap: &Clipmap) -> Vec<u8> {
    let mut out = Vec::new();
    push_rows(&mut out, &columns(camera));
    for level in &clipmap.levels {
        push_rows(&mut out, &columns(&level.view_proj));
    }
    out
}
