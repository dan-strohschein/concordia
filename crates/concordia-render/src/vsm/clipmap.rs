//! The sun's clipmap: levels of virtual shadow maps centred on the camera, each twice as wide as
//! the last, with origins snapped to whole pages so drawn pages keep their place as it moves.
//!
//! References: Tanner, Migdal, Jones, "The Clipmap" (SIGGRAPH 1998); Lefohn, Sengupta, Owens,
//! "Resolution-Matched Shadow Maps" (ACM TOG 2007).

use concordia_core::math::camera::rh::proj::directx;
use concordia_core::math::{IVec2, Mat4, Vec2, Vec3};

use super::table::{PAGES_PER_SIDE, VIRTUAL_TEXELS};
use crate::shadow::light_rotation;

/// Width of level 0 in metres (level `i` is `LEVEL0_EXTENT · 2^i`).
pub const LEVEL0_EXTENT: f32 = 8.0;

/// One clipmap level.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipLevel {
    /// Level index.
    pub index: u32,
    /// World → light clip (reverse-Z orthographic) over the level's square.
    pub view_proj: Mat4,
    origin_page: IVec2,
    camera_light: Vec2,
}

impl ClipLevel {
    /// Width in metres.
    pub fn extent(&self) -> f32 {
        level_extent(self.index)
    }

    /// Width of one page in metres.
    pub fn page_world(&self) -> f32 {
        self.extent() / PAGES_PER_SIDE as f32
    }

    /// The level's corner in light space, in pages.
    pub fn origin_page(&self) -> IVec2 {
        self.origin_page
    }

    /// The level's corner in light space (m).
    pub fn origin_world(&self) -> Vec2 {
        self.origin_page.as_vec2() * self.page_world()
    }

    /// The camera in light space (m).
    pub fn camera_light(&self) -> Vec2 {
        self.camera_light
    }
}

/// Every level of the sun's clipmap for one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Clipmap {
    /// World → light view (rotation only; shared by every level).
    pub light_view: Mat4,
    /// Finest first.
    pub levels: Vec<ClipLevel>,
}

impl Clipmap {
    /// `levels` levels for a light travelling along `light_dir`, centred on `camera`, over the
    /// light-space depths `depth` (z of the farthest and nearest casters, see [`stable_depth_range`]).
    pub fn fit(light_dir: Vec3, camera: Vec3, levels: u8, depth: (f32, f32)) -> Self {
        let light_view = light_rotation(light_dir);
        let eye = light_view.transform_point3(camera);
        let levels = (0..u32::from(levels))
            .map(|index| level(&light_view, eye, index, depth))
            .collect();
        Self { light_view, levels }
    }
}

fn level(light_view: &Mat4, eye: Vec3, index: u32, (z_far, z_near): (f32, f32)) -> ClipLevel {
    let extent = level_extent(index);
    let page = extent / PAGES_PER_SIDE as f32;
    let camera_light = eye.truncate();
    let half = (PAGES_PER_SIDE / 2) as i32;
    let origin_page = (camera_light / page).floor().as_ivec2() - IVec2::splat(half);
    let lo = origin_page.as_vec2() * page;
    let proj = directx::orthographic(lo.x, lo.x + extent, lo.y, lo.y + extent, -z_far, -z_near);
    ClipLevel {
        index,
        view_proj: proj * *light_view,
        origin_page,
        camera_light,
    }
}

/// Width of level `level` in metres.
fn level_extent(level: u32) -> f32 {
    LEVEL0_EXTENT * 2f32.powi(level as i32)
}

/// World size of one texel of level `level`.
pub fn texel_world(level: u32) -> f32 {
    level_extent(level) / VIRTUAL_TEXELS as f32
}

/// World size one pixel covers at `distance` with vertical field of view `fov_y` over `height`
/// pixels (orthographic views pass their height in metres over pixels directly).
pub fn pixel_footprint(distance: f32, fov_y: f32, height: u32) -> f32 {
    distance * 2.0 * (fov_y / 2.0).tan() / height.max(1) as f32
}

/// The finest level whose texels are at least `footprint · 2^bias` wide (at most one shadow texel
/// per screen pixel), clamped to `0..levels`.
pub fn level_for_footprint(footprint: f32, bias: f32, levels: u8) -> u32 {
    let last = u32::from(levels.max(1)) - 1;
    let level = ((footprint / texel_world(0)).log2() + bias).ceil();
    match level.is_finite() {
        true => (level.max(0.0) as u32).min(last),
        false if level > 0.0 => last,
        false => 0,
    }
}

/// A light-space depth range `(far, near)` covering `casters` (their z range): `previous` while the
/// casters fit inside it, else a range of twice the casters' span rounded up to a power of two, on a
/// grid of a quarter of that size, so it changes only when the scene outgrows it.
pub fn stable_depth_range(casters: (f32, f32), previous: Option<(f32, f32)>) -> (f32, f32) {
    let (lo, hi) = (casters.0.min(casters.1), casters.0.max(casters.1));
    if let Some(p) = previous.filter(|p| p.0 <= lo && p.1 >= hi) {
        return p;
    }
    let size = 2f32.powf((2.0 * (hi - lo).max(1.0)).log2().ceil());
    let step = size / 4.0;
    let start = (lo / step).floor() * step;
    (start, start + size)
}
