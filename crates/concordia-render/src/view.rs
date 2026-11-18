//! An explicit viewpoint (the editor's camera) and picking: which mesh is under a pixel.

use concordia_assets::AssetRef;
use concordia_core::id::EntityId;
use concordia_core::math::{Mat3, Mat4, Quat, Vec3};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::{Projection, RenderError, RenderWorld};

/// A viewpoint that is not a scene entity: eye, target and projection (up is +Y).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct View {
    /// Camera position.
    pub eye: Vec3,
    /// Point looked at.
    pub target: Vec3,
    /// Projection.
    pub projection: Projection,
}

impl View {
    /// The camera's world matrix (looking down its -Z at `target`).
    pub fn world_matrix(&self) -> Result<Mat4, RenderError> {
        let forward = self.target - self.eye;
        if !(self.eye.is_finite() && self.target.is_finite()) || forward.length_squared() < 1e-12 {
            return Err(RenderError::InvalidCamera(format!(
                "view eye {} and target {} must differ and be finite",
                self.eye, self.target
            )));
        }
        let f = forward.normalize();
        let up = if f.y.abs() > 0.999 { Vec3::Z } else { Vec3::Y };
        let right = f.cross(up).normalize();
        let rotation = Quat::from_mat3(&Mat3::from_cols(right, right.cross(f), -f));
        Ok(Mat4::from_rotation_translation(rotation, self.eye))
    }
}

/// A pick hit: the entity and its depth along the view direction from the eye.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pick {
    /// The entity.
    pub entity: EntityId,
    /// Distance from the eye along the view direction.
    pub depth: f32,
}

/// The nearest drawn mesh under pixel (x, y) of a `width`×`height` image from `view`, testing each
/// mesh's object-space bounds (`bounds` returns them by mesh reference).
pub fn pick(
    world: &RenderWorld,
    view: &View,
    (x, y): (u32, u32),
    (width, height): (u32, u32),
    mut bounds: impl FnMut(&AssetRef) -> Option<(Vec3, Vec3)>,
) -> Result<Option<Pick>, RenderError> {
    if width == 0 || height == 0 || x >= width || y >= height {
        return Err(RenderError::InvalidSize {
            width,
            height,
            max: crate::MAX_SIZE,
        });
    }
    let camera = view.world_matrix()?;
    let inverse =
        (view.projection.matrix(width as f32 / height as f32)? * camera.inverse()).inverse();
    let ndc = (
        2.0 * (x as f32 + 0.5) / width as f32 - 1.0,
        1.0 - 2.0 * (y as f32 + 0.5) / height as f32,
    );
    let near = inverse.project_point3(Vec3::new(ndc.0, ndc.1, 1.0));
    let far = inverse.project_point3(Vec3::new(ndc.0, ndc.1, 0.0));
    let forward = (view.target - view.eye).normalize();
    let mut best: Option<Pick> = None;
    for draw in &world.draws {
        let Some(aabb) = bounds(&draw.mesh) else {
            continue;
        };
        let to_object = draw.world.inverse();
        let (o, e) = (
            to_object.transform_point3(near),
            to_object.transform_point3(far),
        );
        if let Some(t) = ray_box(o, e - o, aabb) {
            let hit = near + (far - near) * t;
            let depth = (hit - view.eye).dot(forward);
            if best.is_none_or(|b| depth < b.depth) {
                best = Some(Pick {
                    entity: draw.entity,
                    depth,
                });
            }
        }
    }
    Ok(best)
}

/// Slab test: the smallest t in [0, 1] where `origin + dir·t` enters the box.
fn ray_box(origin: Vec3, dir: Vec3, (lo, hi): (Vec3, Vec3)) -> Option<f32> {
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for axis in 0..3 {
        let (o, d) = (origin[axis], dir[axis]);
        if d.abs() < 1e-12 {
            if o < lo[axis] || o > hi[axis] {
                return None;
            }
            continue;
        }
        let (a, b) = ((lo[axis] - o) / d, (hi[axis] - o) / d);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
}
