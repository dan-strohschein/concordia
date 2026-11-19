//! Cascaded shadow maps for the directional light: practical splits, per-cascade bounding
//! spheres snapped to the shadow texel grid, and depth ranges that cover the whole scene.
//!
//! References: Zhang et al., "Parallel-Split Shadow Maps" (2006); Michal Valient, "Stable
//! Rendering of Cascaded Shadow Maps" (ShaderX6); Holbert, "Saying goodbye to shadow acne"
//! (GDC 2011 poster).

use concordia_core::math::camera::rh::proj::directx;
use concordia_core::math::{Mat3, Mat4, Vec3};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::{Projection, RenderError, view_matrix};

/// Percentage-closer filter size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum PcfKernel {
    /// 3×3 texels (tent-filtered).
    #[default]
    X3,
    /// 5×5 texels (softer).
    X5,
}

impl PcfKernel {
    /// Kernel radius in texels.
    pub fn radius(self) -> u32 {
        match self {
            Self::X3 => 1,
            Self::X5 => 2,
        }
    }
}

/// Shadow settings of a directional light.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct CascadeSettings {
    /// Cascades (1..=4).
    #[reflect(range(min = 1.0, max = 4.0))]
    pub cascades: u8,
    /// Shadows end this far from the camera (view distance).
    #[reflect(range(min = 0.1, max = 10000.0))]
    pub max_distance: f32,
    /// Blend between logarithmic (1) and uniform (0) splits.
    #[reflect(range(min = 0.0, max = 1.0))]
    pub split_lambda: f32,
    /// Shadow map size per cascade (texels per side).
    #[reflect(range(min = 64.0, max = 8192.0))]
    pub resolution: u32,
    /// Constant depth bias in world units.
    #[reflect(range(min = 0.0, max = 1.0))]
    pub depth_bias: f32,
    /// Receiver offset along the normal, in shadow texels.
    #[reflect(range(min = 0.0, max = 10.0))]
    pub normal_bias: f32,
    /// Filter.
    pub pcf: PcfKernel,
    /// Snap cascades to the texel grid so shadows do not shimmer while the camera moves.
    pub stabilize: bool,
}

impl Default for CascadeSettings {
    fn default() -> Self {
        Self {
            cascades: 3,
            max_distance: 60.0,
            split_lambda: 0.75,
            resolution: 2048,
            depth_bias: 0.02,
            normal_bias: 1.5,
            pcf: PcfKernel::X3,
            stabilize: true,
        }
    }
}

impl CascadeSettings {
    fn validate(&self) -> Result<(), RenderError> {
        let bad = |why: &str| {
            Err(RenderError::InvalidCamera(format!(
                "shadow settings: {why} ({self:?})"
            )))
        };
        match () {
            _ if !(1..=4).contains(&self.cascades) => bad("cascades must be 1..=4"),
            _ if !(64..=8192).contains(&self.resolution) => bad("resolution must be 64..=8192"),
            _ if !(0.0..=1.0).contains(&self.split_lambda) => bad("split_lambda must be in [0, 1]"),
            _ if !(self.max_distance > 0.0 && self.max_distance.is_finite()) => {
                bad("max_distance must be positive")
            }
            _ if !(self.depth_bias >= 0.0 && self.normal_bias >= 0.0) => {
                bad("biases must be non-negative")
            }
            _ => Ok(()),
        }
    }
}

/// One fitted cascade.
#[derive(Debug, Clone, PartialEq)]
pub struct Cascade {
    /// Far view distance this cascade covers.
    pub split_far: f32,
    /// World → light clip (reverse-Z orthographic).
    pub view_proj: Mat4,
    /// Bounding-sphere radius of the view slice (half the cascade's extent).
    pub radius: f32,
    /// World size of one shadow texel.
    pub texel_world: f32,
    /// Cascade center in light space (snapped to the texel grid when stabilizing).
    pub origin_light: Vec3,
    /// Light-space depth covered, in world units (for converting biases).
    pub depth_range: f32,
    /// The view slice's corners in world space.
    pub slice_corners: [Vec3; 8],
}

/// Far distances of `n` cascades over [near, far]: λ·log + (1-λ)·uniform.
pub fn split_distances(near: f32, far: f32, n: u8, lambda: f32) -> Vec<f32> {
    (1..=n)
        .map(|i| {
            let t = f32::from(i) / f32::from(n);
            lambda * near * (far / near).powf(t) + (1.0 - lambda) * (near + (far - near) * t)
        })
        .collect()
}

/// Fits cascades for a camera (`camera_world` is its entity's world matrix) and a light
/// travelling along `light_dir`; `scene` (min, max) bounds every shadow caster.
pub fn fit_cascades(
    settings: &CascadeSettings,
    projection: &Projection,
    aspect: f32,
    camera_world: &Mat4,
    light_dir: Vec3,
    scene: (Vec3, Vec3),
) -> Result<Vec<Cascade>, RenderError> {
    settings.validate()?;
    projection.matrix(aspect)?;
    let (near, far) = match *projection {
        Projection::Perspective { near, far, .. } | Projection::Orthographic { near, far, .. } => {
            (near, far.min(settings.max_distance.max(near * 1.01)))
        }
    };
    let light = light_rotation(light_dir);
    let to_world = view_matrix(camera_world).inverse();
    let splits = split_distances(near, far, settings.cascades, settings.split_lambda);
    let mut start = near;
    let mut out = Vec::with_capacity(splits.len());
    for split in splits {
        let corners =
            slice_corners(projection, aspect, start, split).map(|c| to_world.transform_point3(c));
        out.push(fit_one(settings, &light, corners, split, scene));
        start = split;
    }
    Ok(out)
}

/// World → light view (rotation only, so the texel grid is fixed in the world).
fn light_rotation(dir: Vec3) -> Mat4 {
    let f = dir.normalize_or(Vec3::NEG_Y);
    let up = if f.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
    let r = f.cross(up).normalize();
    let u = r.cross(f);
    Mat4::from_mat3(Mat3::from_cols(r, u, -f).transpose())
}

/// View-space corners of the slice between view distances `z0` and `z1`.
fn slice_corners(projection: &Projection, aspect: f32, z0: f32, z1: f32) -> [Vec3; 8] {
    let half = |z: f32| match *projection {
        Projection::Perspective { fov_y, .. } => {
            let t = (fov_y / 2.0).tan() * z;
            (t * aspect, t)
        }
        Projection::Orthographic { height, .. } => (height * aspect / 2.0, height / 2.0),
    };
    let mut corners = [Vec3::ZERO; 8];
    for (i, z) in [z0, z1].into_iter().enumerate() {
        let (w, h) = half(z);
        for (j, (sx, sy)) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
            .into_iter()
            .enumerate()
        {
            corners[i * 4 + j] = Vec3::new(sx * w, sy * h, -z);
        }
    }
    corners
}

fn fit_one(
    settings: &CascadeSettings,
    light: &Mat4,
    corners: [Vec3; 8],
    split_far: f32,
    scene: (Vec3, Vec3),
) -> Cascade {
    let center = corners.iter().copied().sum::<Vec3>() / 8.0;
    let mut radius = corners
        .iter()
        .map(|c| c.distance(center))
        .fold(0.0f32, f32::max);
    if settings.stabilize {
        radius = (radius * 16.0).ceil() / 16.0;
    }
    let texel_world = 2.0 * radius / settings.resolution as f32;
    let mut origin = light.transform_point3(center);
    if settings.stabilize {
        origin.x = (origin.x / texel_world).round() * texel_world;
        origin.y = (origin.y / texel_world).round() * texel_world;
    }
    let (mut zmin, mut zmax) = (origin.z - radius, origin.z + radius);
    for i in 0..8 {
        let p = Vec3::new(
            if i & 1 == 0 { scene.0.x } else { scene.1.x },
            if i & 2 == 0 { scene.0.y } else { scene.1.y },
            if i & 4 == 0 { scene.0.z } else { scene.1.z },
        );
        let z = light.transform_point3(p).z;
        zmin = zmin.min(z);
        zmax = zmax.max(z);
    }
    let margin = 0.5;
    let (near_d, far_d) = (-(zmax + margin), -(zmin - margin));
    let proj = directx::orthographic(
        origin.x - radius,
        origin.x + radius,
        origin.y - radius,
        origin.y + radius,
        far_d,
        near_d,
    );
    Cascade {
        split_far,
        view_proj: proj * *light,
        radius,
        texel_world,
        origin_light: origin,
        depth_range: far_d - near_d,
        slice_corners: corners,
    }
}
