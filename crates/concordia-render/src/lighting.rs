//! Punctual lights for the GPU: shading rows (world space, exposed radiance) and view-space
//! bounds for clustering.

use concordia_core::math::{Mat4, Vec3, Vec4};

use crate::cluster::LightBounds;
use crate::frame::push_rows;
use crate::{PunctualKind, RenderWorld};

/// Packed lights and their clustering bounds.
pub(crate) struct LightData {
    /// Four `float4` rows per light: position+range, direction, radiance, spot scale/offset.
    pub(crate) rows: Vec<u8>,
    /// View-space bounding spheres, in the same order.
    pub(crate) bounds: Vec<LightBounds>,
    /// Lights beyond the frame budget (not drawn).
    pub(crate) dropped: usize,
}

pub use concordia_render_types::candela;

/// Packs at most `max` punctual lights, in entity order.
pub(crate) fn punctual_lights(
    world: &RenderWorld,
    view: &Mat4,
    exposure: f32,
    max: usize,
) -> LightData {
    let mut data = LightData {
        rows: Vec::new(),
        bounds: Vec::new(),
        dropped: world.punctual.len().saturating_sub(max),
    };
    for light in world.punctual.iter().take(max) {
        let radiance = Vec3::from_array(light.color) * candela(light.intensity_lumens) * exposure;
        // Spot attenuation is saturate(cos * scale + offset)²; points use scale 0, offset 1.
        let (scale, offset) = match light.kind {
            PunctualKind::Point => (0.0, 1.0),
            PunctualKind::Spot { inner, outer } => {
                let (ci, co) = (inner.cos(), outer.cos());
                let scale = 1.0 / (ci - co).max(1e-4);
                (scale, -co * scale)
            }
        };
        push_rows(
            &mut data.rows,
            &[
                light.position.extend(light.range),
                light.direction.extend(0.0),
                radiance.extend(0.0),
                Vec4::new(scale, offset, 0.0, 0.0),
            ],
        );
        data.bounds.push(LightBounds {
            center: view.transform_point3(light.position),
            radius: light.range,
        });
    }
    data
}
