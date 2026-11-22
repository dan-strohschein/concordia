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

    /// Pixel position of `point` in a `width`×`height` image, or `None` if it is not in view depth.
    pub fn project(&self, point: Vec3, width: f32, height: f32) -> Option<[f32; 2]> {
        if !(width > 0.0 && height > 0.0) {
            return None;
        }
        let clip_from_world =
            self.projection.matrix(width / height).ok()? * self.world_matrix().ok()?.inverse();
        let clip = clip_from_world * point.extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        if !(0.0..=1.0).contains(&ndc.z) {
            return None;
        }
        Some([(ndc.x + 1.0) / 2.0 * width, (1.0 - ndc.y) / 2.0 * height])
    }
}

/// World distance to move along `axis` (from `origin`) for a pointer `delta` in pixels, using the
/// axis' on-screen direction; 0 when the axis points at the eye or is out of view.
pub fn axis_delta(
    view: &View,
    origin: Vec3,
    axis: Vec3,
    delta: [f32; 2],
    width: f32,
    height: f32,
) -> f32 {
    let (Some(a), Some(b)) = (
        view.project(origin, width, height),
        view.project(origin + axis, width, height),
    ) else {
        return 0.0;
    };
    let s = [b[0] - a[0], b[1] - a[1]];
    let length2 = s[0] * s[0] + s[1] * s[1];
    if length2 < 1e-6 {
        return 0.0;
    }
    (delta[0] * s[0] + delta[1] * s[1]) / length2
}

/// Rotation (radians, right-handed about `axis`) for a pointer moved from `from` to `to`: the
/// angle swept around the entity's screen position, counter-clockwise on screen being positive
/// when the axis points at the eye; 0 when a point is on the entity or it is out of view.
pub fn rotate_angle(
    view: &View,
    origin: Vec3,
    axis: Vec3,
    from: [f32; 2],
    to: [f32; 2],
    width: f32,
    height: f32,
) -> f32 {
    let Some(o) = view.project(origin, width, height) else {
        return 0.0;
    };
    let (a, b) = (
        [from[0] - o[0], from[1] - o[1]],
        [to[0] - o[0], to[1] - o[1]],
    );
    if a[0].hypot(a[1]) < 1e-3 || b[0].hypot(b[1]) < 1e-3 {
        return 0.0;
    }
    // Screen y points down, so a positive cross product is clockwise on screen.
    let clockwise = (a[0] * b[1] - a[1] * b[0]).atan2(a[0] * b[0] + a[1] * b[1]);
    let facing = if axis.dot(view.eye - origin) < 0.0 {
        -1.0
    } else {
        1.0
    };
    -clockwise * facing
}

/// Scale factor along `axis` for a pointer `delta` (pixels): 1 plus the drag along the axis'
/// on-screen direction in arrow lengths, at least 0.01; 1 when the axis points at the eye.
pub fn scale_factor(
    view: &View,
    origin: Vec3,
    axis: Vec3,
    delta: [f32; 2],
    width: f32,
    height: f32,
) -> f32 {
    let (Some(a), Some(b)) = (
        view.project(origin, width, height),
        view.project(origin + axis, width, height),
    ) else {
        return 1.0;
    };
    let s = [b[0] - a[0], b[1] - a[1]];
    let length = s[0].hypot(s[1]);
    if length < 1e-3 {
        return 1.0;
    }
    let along = (delta[0] * s[0] + delta[1] * s[1]) / length;
    (1.0 + along / crate::overlay::ARROW).max(0.01)
}

/// A pick hit: the entity and its depth along the view direction from the eye.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pick {
    /// The entity.
    pub entity: EntityId,
    /// Distance from the eye along the view direction.
    pub depth: f32,
    /// Where the pointer's ray enters the mesh's bounds, in world space.
    pub point: Vec3,
}

/// Where something dropped at a pixel lands: on the mesh under it, else on the ground (y = 0),
/// else (a ray that never meets the ground) at the view's focus distance along the ray.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Landing {
    /// The landing point, in world space.
    pub point: Vec3,
    /// The entity landed on, if any.
    pub entity: Option<EntityId>,
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
    let (near, far) = pixel_ray(view, (x, y), (width, height))?;
    let forward = (view.target - view.eye).normalize();
    let mut best: Option<Pick> = None;
    // What this view draws (representation LOD, ADR-0076): hidden entities can't be picked.
    let world = world.represented(view.eye);
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
                    point: hit,
                });
            }
        }
    }
    Ok(best)
}

/// Where an item dropped at pixel (x, y) lands (see [`Landing`]); `bounds` as for [`pick`].
pub fn landing(
    world: &RenderWorld,
    view: &View,
    pixel: (u32, u32),
    size: (u32, u32),
    bounds: impl FnMut(&AssetRef) -> Option<(Vec3, Vec3)>,
) -> Result<Landing, RenderError> {
    if let Some(hit) = pick(world, view, pixel, size, bounds)? {
        return Ok(Landing {
            point: hit.point,
            entity: Some(hit.entity),
        });
    }
    let (near, far) = pixel_ray(view, pixel, size)?;
    let dir = (far - near).normalize_or_zero();
    let ground = (dir.y.abs() > 1e-6)
        .then(|| -near.y / dir.y)
        .filter(|t| *t >= 0.0);
    let point = match ground {
        Some(t) => near + dir * t,
        None => view.eye + dir * (view.target - view.eye).length(),
    };
    Ok(Landing {
        point,
        entity: None,
    })
}

/// The world-space points on the near and far planes under pixel (x, y)'s centre.
fn pixel_ray(
    view: &View,
    (x, y): (u32, u32),
    (width, height): (u32, u32),
) -> Result<(Vec3, Vec3), RenderError> {
    let camera = view.world_matrix()?;
    let inverse =
        (view.projection.matrix(width as f32 / height as f32)? * camera.inverse()).inverse();
    let ndc = (
        2.0 * (x as f32 + 0.5) / width as f32 - 1.0,
        1.0 - 2.0 * (y as f32 + 0.5) / height as f32,
    );
    Ok((
        inverse.project_point3(Vec3::new(ndc.0, ndc.1, 1.0)),
        inverse.project_point3(Vec3::new(ndc.0, ndc.1, 0.0)),
    ))
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
