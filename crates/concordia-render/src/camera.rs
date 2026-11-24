//! View and projection matrices (right-handed, Y-up NDC, reverse-Z, depth in [0, 1]).

use concordia_core::math::{Mat4, Vec3};

pub use concordia_render_types::view_matrix;

/// The camera's world position.
pub(crate) fn eye(world: &Mat4) -> Vec3 {
    world.transform_point3(Vec3::ZERO)
}
