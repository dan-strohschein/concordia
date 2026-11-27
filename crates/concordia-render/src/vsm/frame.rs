//! The per-frame VSM blocks: the parameter rows (`VsmParams` in `vsm.slang`) and the depth views
//! (the camera, then one per clipmap level).

use concordia_core::math::{IVec2, Mat4, Vec3, Vec4};

use super::clipmap::{Clipmap, LEVEL0_EXTENT, texel_world};
use super::settings::VsmSettings;
use crate::frame::{columns, push_rows, push_rows_u32};

/// Row of the first level's origin (`VSM_ROW_LEVELS`).
pub(crate) const ROW_LEVELS: usize = 14;
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
    /// Each level's origin (in pages) last frame, for releasing slots scrolled out (4.7b).
    pub(crate) previous: &'a [IVec2],
    /// Bindless indices of the page table, the dynamic table (4.7b) and the local lights' rows
    /// (4.7c; a header row of zeros when there are none).
    pub(crate) tables: [u32; 3],
    /// Whether the sun casts (its pages are marked; 4.7c).
    pub(crate) sun: bool,
    /// Soft shadows (4.7d): samples, the sun's angular radius, the noise's frame (0: fixed).
    pub(crate) soft: (u32, f32, u32),
}

/// The coarse floor: the finest level at least eight times `max_distance` wide, so a frame's
/// floor pages stay few (at most a 16-page radius around the camera), clamped to `levels`.
pub fn floor_level(max_distance: f32, levels: u8) -> u32 {
    let need = (8.0 * max_distance / LEVEL0_EXTENT).max(1.0).log2().ceil();
    (need as u32).min(u32::from(levels.max(1)) - 1)
}

/// The parameter rows (mirrors `VsmParams`, `vsm_origin` and `vsm_previous_origin` in `vsm.slang`):
/// the fixed rows, each level's origin, then each level's origin last frame.
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
    push_rows_u32(
        &mut out,
        &[[p.tables[0], p.tables[1], p.tables[2], u32::from(p.sun)]],
    );
    push_rows_u32(&mut out, &[[p.soft.0, p.soft.1.to_bits(), p.soft.2, 0]]);
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
    debug_assert_eq!(p.previous.len(), p.clipmap.levels.len(), "previous origins");
    let previous: Vec<[u32; 4]> = p
        .previous
        .iter()
        .map(|o| [o.x as u32, o.y as u32, 0, 0])
        .collect();
    push_rows_u32(&mut out, &previous);
    debug_assert_eq!(out.len(), (ROW_LEVELS + 2 * p.clipmap.levels.len()) * 16);
    out
}

/// The depth views: the camera's view-projection, each level's (`cascade_xform` rows), the light
/// view (`vsm_page_position`), then the local maps' (4.7c; view `2 + levels + map`).
pub(crate) fn depth_views(camera: &Mat4, clipmap: &Clipmap, locals: &[Mat4]) -> Vec<u8> {
    let mut out = Vec::new();
    push_rows(&mut out, &columns(camera));
    for level in &clipmap.levels {
        push_rows(&mut out, &columns(&level.view_proj));
    }
    push_rows(&mut out, &columns(&clipmap.light_view));
    for m in locals {
        push_rows(&mut out, &columns(m));
    }
    out
}
