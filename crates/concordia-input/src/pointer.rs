//! Pointer rays: from a cursor position through the camera into the world.

use concordia_core::math::{Mat4, Ray, Vec2, Vec3};

use crate::error::InputError;

/// The world ray under `cursor` (logical pixels, origin top-left) for a reverse-Z, Y-up
/// `inv_view_proj`; works for perspective and orthographic cameras.
pub fn pointer_ray(cursor: Vec2, viewport: Vec2, inv_view_proj: Mat4) -> Result<Ray, InputError> {
    if !cursor.is_finite() || !viewport.is_finite() || viewport.cmple(Vec2::ZERO).any() {
        return Err(InputError::Pointer(format!(
            "cursor {cursor} in viewport {viewport}: both must be finite and the viewport positive"
        )));
    }
    let ndc = Vec2::new(
        2.0 * cursor.x / viewport.x - 1.0,
        1.0 - 2.0 * cursor.y / viewport.y,
    );
    let near = inv_view_proj.project_point3(Vec3::new(ndc.x, ndc.y, 1.0));
    let mid = inv_view_proj.project_point3(Vec3::new(ndc.x, ndc.y, 0.5));
    Ray::new(near, mid - near).map_err(|e| InputError::Pointer(e.to_string()))
}
