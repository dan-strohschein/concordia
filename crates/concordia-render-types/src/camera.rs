//! Projection and view matrices (right-handed, Y-up NDC, reverse-Z, depth in [0, 1]).

use concordia_core::id::EntityId;
use concordia_core::math::Mat4;
use concordia_core::math::camera::rh::proj::directx;

use crate::Projection;

/// Why a projection cannot make a matrix.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ProjectionError(pub String);

impl Projection {
    /// The projection matrix for `aspect` (width / height); near maps to depth 1, far to 0.
    pub fn matrix(&self, aspect: f32) -> Result<Mat4, ProjectionError> {
        let bad = |why: &str| {
            Err(ProjectionError(format!(
                "{why} ({self:?}, aspect {aspect})"
            )))
        };
        if !(aspect.is_finite() && aspect > 0.0) {
            return bad("aspect must be positive");
        }
        match *self {
            Self::Perspective { fov_y, near, far } => {
                if !(fov_y > 0.0 && fov_y < std::f32::consts::PI) {
                    return bad("fov_y must be in (0, π)");
                }
                if !(near > 0.0 && far > near && far.is_finite()) {
                    return bad("perspective needs 0 < near < far");
                }
                Ok(directx::perspective(fov_y, aspect, far, near))
            }
            Self::Orthographic { height, near, far } => {
                if !(height > 0.0 && height.is_finite()) {
                    return bad("height must be positive");
                }
                if !(near >= 0.0 && far > near && far.is_finite()) {
                    return bad("orthographic needs 0 ≤ near < far");
                }
                let (h, w) = (height / 2.0, height * aspect / 2.0);
                Ok(directx::orthographic(-w, w, -h, h, far, near))
            }
        }
    }
}

/// The view matrix of a camera entity's world matrix (scale removed; looks down -Z).
pub fn view_matrix(world: &Mat4) -> Mat4 {
    let (_, rotation, translation) = world.to_scale_rotation_translation();
    Mat4::from_rotation_translation(rotation, translation).inverse()
}

/// The camera a frame uses: `requested` if it is among `cameras` (entity, priority), else, with
/// no request, the highest priority, ties going to the lowest entity id.
pub fn pick_camera(
    cameras: impl IntoIterator<Item = (EntityId, i32)>,
    requested: Option<EntityId>,
) -> Option<EntityId> {
    let mut cameras = cameras.into_iter();
    match requested {
        Some(id) => cameras.find(|(e, _)| *e == id).map(|(e, _)| e),
        None => cameras
            .max_by_key(|(e, priority)| (*priority, std::cmp::Reverse(*e)))
            .map(|(e, _)| e),
    }
}
